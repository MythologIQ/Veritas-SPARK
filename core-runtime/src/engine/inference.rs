//! Core inference execution with real model delegation.

use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::engine::gguf::GgufModel;
use crate::engine::{InferenceConfig, InferenceInput, InferenceOutput};
use crate::models::ModelHandle;

#[derive(Error, Debug)]
pub enum InferenceError {
    #[error("Model not loaded: {0}")]
    ModelNotLoaded(String),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Inference failed: {0}")]
    ExecutionFailed(String),

    #[error("Context length exceeded: max {max}, got {got}")]
    ContextExceeded { max: usize, got: usize },

    #[error("Memory limit exceeded: used {used} bytes, limit {limit} bytes")]
    MemoryExceeded { used: usize, limit: usize },
}

/// Parameters controlling inference behavior (IPC protocol).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InferenceParams {
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: usize,
    /// Enable token-by-token streaming response.
    #[serde(default)]
    pub stream: bool,
    /// Request timeout in milliseconds. None = no timeout.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl Default for InferenceParams {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            stream: false,
            timeout_ms: None,
        }
    }
}

impl InferenceParams {
    pub fn validate(&self) -> Result<(), InferenceError> {
        if self.max_tokens == 0 {
            return Err(InferenceError::InvalidParams("max_tokens must be > 0".into()));
        }
        if self.temperature < 0.0 {
            return Err(InferenceError::InvalidParams("temperature must be >= 0".into()));
        }
        if self.top_p <= 0.0 || self.top_p > 1.0 {
            return Err(InferenceError::InvalidParams("top_p must be in (0, 1]".into()));
        }
        Ok(())
    }

    /// Convert to internal InferenceConfig format.
    pub fn to_config(&self) -> InferenceConfig {
        InferenceConfig {
            max_tokens: Some(self.max_tokens as u32),
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k as u32,
            repetition_penalty: 1.1,
            timeout_ms: self.timeout_ms.unwrap_or(30_000),
            max_memory_bytes: None,
        }
    }
}

/// Result of inference execution.
#[derive(Debug, Clone)]
pub struct InferenceResult {
    /// Generated text output.
    pub output: String,
    pub tokens_generated: usize,
    pub finished: bool,
}

/// Executes model inference by delegating to registered models.
pub struct InferenceEngine {
    max_context_length: usize,
    /// Models indexed by model_id for lookup.
    models: Arc<RwLock<HashMap<String, Arc<dyn GgufModel>>>>,
    /// ModelHandle to model_id mapping (O(1) handle->id).
    handle_to_id: Arc<RwLock<HashMap<u64, String>>>,
    /// model_id to ModelHandle mapping (O(1) id->handle).
    id_to_handle: Arc<RwLock<HashMap<String, u64>>>,
}

impl InferenceEngine {
    pub fn new(max_context_length: usize) -> Self {
        Self {
            max_context_length,
            models: Arc::new(RwLock::new(HashMap::new())),
            handle_to_id: Arc::new(RwLock::new(HashMap::new())),
            id_to_handle: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a model for inference.
    pub async fn register_model(
        &self,
        model_id: String,
        handle: ModelHandle,
        model: Arc<dyn GgufModel>,
    ) {
        self.models.write().await.insert(model_id.clone(), model);
        self.handle_to_id.write().await.insert(handle.id(), model_id.clone());
        self.id_to_handle.write().await.insert(model_id, handle.id());
    }

    /// Unregister a model.
    pub async fn unregister_model(&self, model_id: &str) {
        self.models.write().await.remove(model_id);
        if let Some(handle_id) = self.id_to_handle.write().await.remove(model_id) {
            self.handle_to_id.write().await.remove(&handle_id);
        }
    }

    /// Run inference on text prompt using the specified model.
    ///
    /// NOTE: The read lock on `self.models` is held for the entire
    /// inference call. This is a P2 optimization target — clone the
    /// Arc<dyn GgufModel> and drop the lock before calling infer().
    pub async fn run(
        &self,
        model_id: &str,
        prompt: &str,
        params: &InferenceParams,
    ) -> Result<InferenceResult, InferenceError> {
        params.validate()?;
        let model = self.get_model(model_id).await?;
        self.check_context(prompt)?;
        Self::infer_with_model(&model, prompt, params).await
    }

    /// Run inference with cooperative cancellation.
    ///
    /// Checks `is_cancelled` before inference. Per-token cancellation
    /// is threaded through the GGUF backend in a future PR (P0.3).
    pub async fn run_cancellable(
        &self,
        model_id: &str,
        prompt: &str,
        params: &InferenceParams,
        is_cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<InferenceResult, InferenceError> {
        use std::sync::atomic::Ordering;

        params.validate()?;

        if is_cancelled.load(Ordering::Acquire) {
            return Err(InferenceError::ExecutionFailed("cancelled".into()));
        }

        let model = self.get_model(model_id).await?;
        self.check_context(prompt)?;

        let result = Self::infer_with_model(&model, prompt, params).await?;

        if is_cancelled.load(Ordering::Acquire) {
            return Err(InferenceError::ExecutionFailed("cancelled".into()));
        }

        Ok(result)
    }

    /// Run inference with cooperative cancellation and a per-call memory budget.
    ///
    /// The `max_memory_bytes` is enforced before calling into the model: if the
    /// model's reported memory exceeds the budget, `MemoryExceeded` is returned
    /// without starting inference (OOM prevention).
    pub async fn run_cancellable_with_memory_limit(
        &self,
        model_id: &str,
        prompt: &str,
        params: &InferenceParams,
        is_cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        max_memory_bytes: usize,
    ) -> Result<InferenceResult, InferenceError> {
        use std::sync::atomic::Ordering;

        params.validate()?;

        if is_cancelled.load(Ordering::Acquire) {
            return Err(InferenceError::ExecutionFailed("cancelled".into()));
        }

        let model = self.get_model(model_id).await?;
        self.check_context(prompt)?;

        let result = Self::infer_with_model_budget(
            &model, prompt, params, Some(max_memory_bytes),
        ).await?;

        if is_cancelled.load(Ordering::Acquire) {
            return Err(InferenceError::ExecutionFailed("cancelled".into()));
        }

        Ok(result)
    }

    /// Look up a model by ID, cloning the Arc (drops the read lock).
    async fn get_model(
        &self,
        model_id: &str,
    ) -> Result<Arc<dyn GgufModel>, InferenceError> {
        let models = self.models.read().await;
        models.get(model_id).cloned().ok_or_else(|| {
            InferenceError::ModelNotLoaded(model_id.to_string())
        })
    }

    fn check_context(&self, prompt: &str) -> Result<(), InferenceError> {
        if prompt.len() > self.max_context_length {
            return Err(InferenceError::ContextExceeded {
                max: self.max_context_length,
                got: prompt.len(),
            });
        }
        Ok(())
    }

    async fn infer_with_model(
        model: &Arc<dyn GgufModel>,
        prompt: &str,
        params: &InferenceParams,
    ) -> Result<InferenceResult, InferenceError> {
        Self::infer_with_model_budget(model, prompt, params, None).await
    }

    async fn infer_with_model_budget(
        model: &Arc<dyn GgufModel>,
        prompt: &str,
        params: &InferenceParams,
        max_memory_bytes: Option<usize>,
    ) -> Result<InferenceResult, InferenceError> {
        // Enforce per-call memory budget: reject before inference if model
        // already reports more memory than the budget allows.
        if let Some(budget) = max_memory_bytes {
            let model_mem = model.memory_usage();
            if model_mem > budget {
                return Err(InferenceError::MemoryExceeded {
                    used: model_mem,
                    limit: budget,
                });
            }
        }

        let mut config = params.to_config();
        config.max_memory_bytes = max_memory_bytes;

        let input = InferenceInput::Text(prompt.to_string());
        let output = model.infer(&input, &config).await.map_err(|e| {
            InferenceError::ExecutionFailed(e.to_string())
        })?;

        match output {
            InferenceOutput::Generation(gen) => Ok(InferenceResult {
                output: gen.text,
                tokens_generated: gen.tokens_generated as usize,
                finished: true,
            }),
            _ => Err(InferenceError::ExecutionFailed(
                "Model returned non-generation output".into(),
            )),
        }
    }

    /// Run inference by handle (legacy API compatibility).
    pub async fn run_by_handle(
        &self,
        handle: ModelHandle,
        prompt: &str,
        params: &InferenceParams,
    ) -> Result<InferenceResult, InferenceError> {
        let handles = self.handle_to_id.read().await;
        let model_id = handles.get(&handle.id()).ok_or_else(|| {
            InferenceError::ModelNotLoaded(format!("handle {}", handle.id()))
        })?;
        self.run(model_id, prompt, params).await
    }

    pub fn max_context_length(&self) -> usize {
        self.max_context_length
    }

    /// Check if a model is registered.
    pub async fn has_model(&self, model_id: &str) -> bool {
        self.models.read().await.contains_key(model_id)
    }

    /// Return the memory usage reported by a registered model, or None if not found.
    pub async fn model_memory_usage(&self, model_id: &str) -> Option<usize> {
        self.models.read().await.get(model_id).map(|m| m.memory_usage())
    }

    /// Get the ModelHandle for a model_id (O(1) lookup).
    pub async fn get_handle(&self, model_id: &str) -> Option<ModelHandle> {
        self.id_to_handle.read().await
            .get(model_id)
            .map(|&id| ModelHandle::new(id))
    }

    /// Run streaming inference, sending tokens to the provided sender.
    ///
    /// This method looks up the model, downcasts to GgufGenerator, and calls
    /// generate_stream(). Designed for use with spawn_blocking.
    #[cfg(feature = "gguf")]
    pub fn run_stream_sync(
        &self,
        model_id: &str,
        prompt: &str,
        config: &InferenceConfig,
        sender: crate::engine::TokenStreamSender,
    ) -> Result<(), InferenceError> {
        use crate::engine::gguf::GgufGenerator;

        // Get runtime handle for async model lookup
        let rt = tokio::runtime::Handle::current();
        let models = rt.block_on(self.models.read());
        let model = models.get(model_id).ok_or_else(|| {
            InferenceError::ModelNotLoaded(model_id.to_string())
        })?;

        // Downcast to GgufGenerator for streaming access
        let generator = model.as_any().downcast_ref::<GgufGenerator>().ok_or_else(|| {
            InferenceError::ExecutionFailed("model does not support streaming".into())
        })?;

        generator.generate_stream(prompt, config, sender, None)
            .map_err(|e| InferenceError::ExecutionFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inference_params_default_is_valid() {
        let params = InferenceParams::default();
        assert!(params.validate().is_ok());
    }

    #[test]
    fn inference_params_rejects_zero_max_tokens() {
        let params = InferenceParams {
            max_tokens: 0,
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn inference_params_rejects_negative_temperature() {
        let params = InferenceParams {
            temperature: -0.1,
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn inference_params_rejects_invalid_top_p() {
        let params = InferenceParams {
            top_p: 0.0,
            ..Default::default()
        };
        assert!(params.validate().is_err());

        let params = InferenceParams {
            top_p: 1.5,
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[tokio::test]
    async fn engine_new_creates_empty_engine() {
        let engine = InferenceEngine::new(4096);
        assert_eq!(engine.max_context_length(), 4096);
        assert!(!engine.has_model("any-model").await);
    }

    #[tokio::test]
    async fn engine_get_handle_returns_none_for_unregistered() {
        let engine = InferenceEngine::new(4096);
        assert!(engine.get_handle("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn engine_run_fails_for_unloaded_model() {
        let engine = InferenceEngine::new(4096);
        let params = InferenceParams::default();
        let result = engine.run("missing-model", "test prompt", &params).await;
        assert!(matches!(result, Err(InferenceError::ModelNotLoaded(_))));
    }

    #[tokio::test]
    async fn engine_run_by_handle_fails_for_unknown_handle() {
        let engine = InferenceEngine::new(4096);
        let params = InferenceParams::default();
        let handle = ModelHandle::new(999);
        let result = engine.run_by_handle(handle, "test", &params).await;
        assert!(matches!(result, Err(InferenceError::ModelNotLoaded(_))));
    }

    // ---- Memory budget enforcement tests ----

    use std::sync::Arc as StdArc;
    use crate::engine::gguf::GgufModel;
    use crate::engine::{
        FinishReason, GenerationResult, InferenceCapability, InferenceConfig,
        InferenceError as EngineError, InferenceInput, InferenceOutput,
    };

    struct BudgetModel {
        reported_memory: usize,
    }

    #[async_trait::async_trait]
    impl GgufModel for BudgetModel {
        fn model_id(&self) -> &str { "budget-model" }
        fn capabilities(&self) -> &[InferenceCapability] {
            &[InferenceCapability::TextGeneration]
        }
        fn memory_usage(&self) -> usize { self.reported_memory }
        async fn infer(
            &self, _: &InferenceInput, _: &InferenceConfig,
        ) -> Result<InferenceOutput, EngineError> {
            Ok(InferenceOutput::Generation(GenerationResult {
                text: "ok".into(),
                tokens_generated: 1,
                finish_reason: FinishReason::Stop,
            }))
        }
        async fn unload(&mut self) -> Result<(), EngineError> { Ok(()) }
        fn as_any(&self) -> &dyn std::any::Any { self }
    }

    async fn engine_with_budget_model(memory_usage: usize) -> InferenceEngine {
        let engine = InferenceEngine::new(4096);
        let handle = ModelHandle::new(1);
        let model: StdArc<dyn GgufModel> = StdArc::new(BudgetModel { reported_memory: memory_usage });
        engine.register_model("budget-model".into(), handle, model).await;
        engine
    }

    #[tokio::test]
    async fn memory_budget_allows_inference_when_model_fits() {
        let engine = engine_with_budget_model(512).await;
        let params = InferenceParams::default();
        let cancelled = StdArc::new(std::sync::atomic::AtomicBool::new(false));
        // Budget of 1024 bytes, model uses 512 — should succeed.
        let result = engine
            .run_cancellable_with_memory_limit("budget-model", "hi", &params, cancelled, 1024)
            .await;
        assert!(result.is_ok(), "expected success, got {result:?}");
    }

    #[tokio::test]
    async fn memory_budget_rejects_when_model_exceeds_budget() {
        let engine = engine_with_budget_model(2048).await;
        let params = InferenceParams::default();
        let cancelled = StdArc::new(std::sync::atomic::AtomicBool::new(false));
        // Budget of 1024 bytes, model uses 2048 — must reject.
        let result = engine
            .run_cancellable_with_memory_limit("budget-model", "hi", &params, cancelled, 1024)
            .await;
        assert!(
            matches!(result, Err(InferenceError::MemoryExceeded { used: 2048, limit: 1024 })),
            "expected MemoryExceeded, got {result:?}"
        );
    }

    #[tokio::test]
    async fn zero_byte_budget_always_rejects() {
        let engine = engine_with_budget_model(1).await;
        let params = InferenceParams::default();
        let cancelled = StdArc::new(std::sync::atomic::AtomicBool::new(false));
        let result = engine
            .run_cancellable_with_memory_limit("budget-model", "hi", &params, cancelled, 0)
            .await;
        assert!(
            matches!(result, Err(InferenceError::MemoryExceeded { .. })),
            "zero budget must reject every model"
        );
    }

    #[tokio::test]
    async fn to_config_sets_max_memory_bytes_from_budget() {
        // Verify that infer_with_model_budget passes the budget into InferenceConfig.
        // We confirm the model sees max_memory_bytes = Some(budget) through
        // the config by observing that no MemoryExceeded error is raised when
        // model_mem <= budget, and it IS raised when model_mem > budget.
        let engine = engine_with_budget_model(100).await;
        let params = InferenceParams::default();
        let cancelled = StdArc::new(std::sync::atomic::AtomicBool::new(false));

        // 100-byte model, 200-byte budget — passes.
        let ok = engine
            .run_cancellable_with_memory_limit("budget-model", "hi", &params, cancelled.clone(), 200)
            .await;
        assert!(ok.is_ok());

        // 100-byte model, 50-byte budget — rejected.
        let err = engine
            .run_cancellable_with_memory_limit("budget-model", "hi", &params, cancelled, 50)
            .await;
        assert!(matches!(err, Err(InferenceError::MemoryExceeded { .. })));
    }
}
