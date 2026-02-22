# GG-CORE Scalability Remediation and Upgrade Path

**Version:** 1.0
**Date:** 2026-02-22
**Status:** planned
**Audience:** Runtime engineering, platform engineering, release engineering

## Scope

This document defines a full remediation and upgrade path to make GG-CORE operationally correct at current scale, then reliable for large and extremely large model workloads.

It is based on current code evidence in this repository.

## Executive Summary

GG-CORE has strong architectural intent for scalable serving (paging, model pooling, smart loading, multi-GPU, cancellation, metrics). Several core paths are not fully integrated, which blocks dependable scale-out and large-model operations.

The upgrade path is:

1. Restore runtime correctness and serving integrity.
2. Bind resource controls to real inference execution.
3. Deliver large-model readiness (GPU, model lifecycle, queueing, config fidelity).
4. Harden for extremely large models and high concurrency with staged gates.

## Claim Map

| Claim                                                                    | Status      | Source                                                                          |
| ------------------------------------------------------------------------ | ----------- | ------------------------------------------------------------------------------- |
| Runtime enqueues inference requests before execution                     | implemented | `core-runtime/src/ipc/handler.rs:232`                                           |
| Runtime warmup path also enqueues only                                   | implemented | `core-runtime/src/ipc/handler.rs:289`                                           |
| Queue has `dequeue` API                                                  | implemented | `core-runtime/src/scheduler/queue.rs:148`                                       |
| No dequeue callsites in runtime sources                                  | implemented | `core-runtime/src/scheduler/queue.rs:148`                                       |
| Inference engine requires registered models for execution                | implemented | `core-runtime/src/engine/inference.rs:129`                                      |
| Inference model registration API exists but is not wired in runtime      | implemented | `core-runtime/src/engine/inference.rs:111`                                      |
| FFI model load registers metadata with memory `0` only                   | implemented | `core-runtime/src/ffi/models.rs:49`                                             |
| Runtime default max context is hardcoded to 4096                         | implemented | `core-runtime/src/main.rs:463`                                                  |
| Runtime `RuntimeConfig` default max context is 4096                      | implemented | `core-runtime/src/lib.rs:101`                                                   |
| Helm sets `GG_CORE_MAX_CONTEXT`, but runtime load path hardcodes context | implemented | `k8s/helm/gg-core/templates/deployment.yaml:29`, `core-runtime/src/main.rs:456` |
| GGUF default config is CPU-first (`n_gpu_layers: 0`) and `n_ctx: 2048`   | implemented | `core-runtime/src/engine/gguf/mod.rs:38`                                        |
| Input max text size is 64KB                                              | implemented | `core-runtime/src/engine/input.rs:9`                                            |
| Scheduler batch defaults are limited (`8` requests, `4096` est tokens)   | implemented | `core-runtime/src/scheduler/batch.rs:15`                                        |
| IPC frame and message hard limits are 16MB                               | implemented | `core-runtime/src/ipc/server.rs:24`, `core-runtime/src/ipc/protocol.rs:365`     |
| Resource limit enforcement type exists (`ResourceLimits`)                | implemented | `core-runtime/src/memory/limits.rs:57`                                          |
| Inference param conversion disables per-call max memory (`None`)         | implemented | `core-runtime/src/engine/inference.rs:78`                                       |
| GPU allocation path currently uses null pointers/placeholders            | implemented | `core-runtime/src/engine/gpu.rs:297`                                            |
| Multi-GPU P2P path currently includes assumed support comment            | implemented | `core-runtime/src/engine/multi_gpu.rs:270`                                      |
| Core CLI `models list` and `config` subcommands are not implemented      | implemented | `core-runtime/src/main.rs:103`, `core-runtime/src/main.rs:117`                  |

## Target End State

- Correct request lifecycle: admission, scheduling, execution, completion, drain.
- Deterministic model lifecycle: load, register, warmup, route, unload.
- Scalable config model: no hidden hardcoded scale ceilings.
- Real resource guardrails active in serving path.
- Verified large-model operation on GPU and multi-GPU paths.
- Reproducible benchmark and reliability gates before production promotion.

## Phase 0: Correctness Recovery (P0)

**Objective:** eliminate architectural breaks that invalidate scale claims.

### Workstream 0.1: Queue-Execution Integration

- Make the queue the single execution arbiter for inference requests.
- Implement a scheduler/worker loop that consumes `RequestQueue::dequeue` and executes inference.
- Remove direct execution path from IPC handler after enqueue to avoid dual-path semantics.
- Replace byte-length context checks with token-aware context checks in execution admission.
- Ensure timeout/cancel/deadline are enforced from queued request metadata.

**Primary files**

- `core-runtime/src/ipc/handler.rs`
- `core-runtime/src/scheduler/queue.rs`
- `core-runtime/src/scheduler/continuous.rs`

**Acceptance gates**

- Queue depth returns to baseline under sustained load (no monotonic growth).
- `QueueFull` occurs only at configured saturation, not due to leaked queue entries.
- Cancellation works for queued and actively running requests.

### Workstream 0.2: Model Lifecycle Wiring

- Create a single load/register path: validate model path -> load GGUF/ONNX backend -> register in `InferenceEngine` and `ModelRegistry`.
- Ensure registry and inference state updates are atomic to prevent partial load states.
- Merge registries or maintain synchronized bidirectional indexes as one logical source of truth.
- Ensure unload unregisters from both registries and frees backend resources.
- Replace metadata-only load behavior in FFI with true runtime load behavior.

**Primary files**

- `core-runtime/src/engine/inference.rs`
- `core-runtime/src/engine/gguf/mod.rs`
- `core-runtime/src/ffi/models.rs`
- `core-runtime/src/models/preload.rs`

**Acceptance gates**

- Inference after load succeeds without out-of-band test-only model setup.
- `models` diagnostics show accurate memory and state transitions.
- Unload removes model from both inference and metadata paths.

### Workstream 0.3: API Surface Repair (FFI/Python)

- Align FFI/Python inference calls with current `InferenceEngine` signature (string model ID + prompt API or formal token API adapter).
- Remove stale assumptions about `output_tokens` if output is text-first.
- Add compatibility layer only if needed for stable external contracts.
- Add binding integration tests for load -> infer -> unload parity with native runtime path.

**Primary files**

- `core-runtime/src/ffi/inference.rs`
- `core-runtime/src/python/session.rs`
- `core-runtime/src/engine/inference.rs`

**Acceptance gates**

- FFI and Python integration tests compile and pass on current API.
- No type/signature mismatch between binding code and engine core.

## Phase 1: Config and Guardrail Activation (P1)

**Objective:** move from hardcoded limits to explicit, audited runtime control.

### Workstream 1.1: Runtime Configuration Fidelity

- Parse and honor context, batching, queue, connection, and memory limits from environment/config.
- Make Helm values map to runtime effective config (no dead env knobs).

**Primary files**

- `core-runtime/src/main.rs`
- `core-runtime/src/lib.rs`
- `k8s/helm/gg-core/templates/deployment.yaml`
- `k8s/helm/gg-core/values.yaml`

**Acceptance gates**

- Runtime reports effective values matching deployment values.
- Changing Helm values changes runtime behavior without code changes.

### Workstream 1.2: Enforce Resource Limits in Hot Path

- Integrate `ResourceLimits::try_acquire` into inference dispatch path.
- Use `InferenceConfig.max_memory_bytes` coherently with request and global budget.
- Return structured errors on admission rejection (memory/concurrency).

**Primary files**

- `core-runtime/src/memory/limits.rs`
- `core-runtime/src/engine/inference.rs`
- `core-runtime/src/ipc/handler.rs`

**Acceptance gates**

- Memory/concurrency caps trigger deterministic rejections, not process-level OOM.
- Metrics distinguish admission rejection vs execution failure.

### Workstream 1.3: CLI Operational Completeness

- Implement `models list`, config inspection, and verification commands.
- Expose scale-critical diagnostics (effective limits, queue stats, scheduler state).

**Primary files**

- `core-runtime/src/main.rs`
- `core-runtime/src/cli/status.rs`
- `core-runtime/src/ipc/protocol.rs`

**Acceptance gates**

- Operators can audit scale configuration and runtime health from CLI only.

## Phase 2: Large-Model Readiness (P2)

**Objective:** reliably operate 7B-70B class deployments with production constraints.

### Workstream 2.1: GPU Path Completion

- Replace placeholder GPU memory allocation with real backend allocations.
- Validate allocation/free lifecycle and OOM behavior per backend.
- Add backend parity checks for CUDA and Metal (where supported).

**Primary files**

- `core-runtime/src/engine/gpu.rs`
- `core-runtime/src/engine/cuda.rs`
- `core-runtime/src/engine/metal.rs`

**Acceptance gates**

- Non-null device allocations; verified deallocation and leak-free steady state.
- End-to-end GPU inference tests pass with controlled memory pressure.

### Workstream 2.2: Multi-GPU Execution Path

- Move from partition planning to execution wiring for model shards/layers.
- Replace assumed P2P logic with capability detection and fallback paths.
- Add explicit behavior for heterogeneous GPU memory/capability fleets.

**Primary files**

- `core-runtime/src/engine/multi_gpu.rs`
- `core-runtime/src/engine/gguf/backend.rs`

**Acceptance gates**

- Verified scale-out throughput on 2+ GPUs for same model/config.
- Correct behavior when P2P is unavailable (host-staging fallback).

### Workstream 2.3: Model Pool and Smart Loader Integration

- Replace placeholder handles in smart loading with real loaded handles.
- Integrate pool/smart-loader decisions with real registry and inference engine.
- Use warmup to generate model-ready caches rather than queue-only no-op.

**Primary files**

- `core-runtime/src/models/smart_loader.rs`
- `core-runtime/src/models/pool.rs`
- `core-runtime/src/ipc/handler.rs`

**Acceptance gates**

- Measured warm-switch latency with real models.
- No stale handles or mismatched registry/inference state.

## Phase 3: Extremely Large Model Path (P3)

**Objective:** support very large models and longer contexts with predictable behavior.

### Workstream 3.1: Long-Context and KV Strategy

- Optimize and harden token-aware context policy introduced in P0.
- Integrate paged KV and prefill/decode budgeting with backend limits.
- Expose per-request context budgets and enforce admission control.

**Primary files**

- `core-runtime/src/engine/inference.rs`
- `core-runtime/src/engine/input.rs`
- `core-runtime/src/memory/paged.rs`
- `core-runtime/src/engine/prefill.rs`

**Acceptance gates**

- Stable long-context runs without unbounded memory growth.
- Predictable latency degradation curves as context grows.

### Workstream 3.2: Distributed and Kubernetes Scale Fidelity

- Align chart examples with actual runtime capabilities and defaults.
- Add production profiles for model-size tiers (small, medium, large, xlarge).
- Add canary guardrails that include queue saturation and memory admission errors.

**Primary files**

- `k8s/helm/gg-core/examples/values-production.yaml`
- `k8s/helm/gg-core/examples/values-multi-gpu.yaml`
- `k8s/helm/gg-core/templates/deployment.yaml`

**Acceptance gates**

- Helm templates and runtime behavior are config-consistent.
- Canary rollouts auto-stop on scale-regression signals.

## Cross-Cutting Test and Release Gates

### Required Benchmark Tiers

1. Functional: load/infer/unload/warmup/cancel under concurrency.
2. Stress: queue saturation, memory cap enforcement, connection cap behavior.
3. Large-model: throughput and tail-latency across GPU counts.
4. Failure: OOM, timeout, drain shutdown, model swap during load.

### Mandatory Promotion Criteria

- No regression in correctness tests introduced in Phase 0.
- All scale knobs report effective values via status endpoints/CLI.
- Documented model-size envelopes with verified hardware baselines.
- Reproducible benchmark scripts and raw artifacts checked into docs/testing artifacts.
- Workstream-level unit/integration tests added for each correctness fix.

## Performance Estimates and Stage Gates

### Baseline and Estimation Method

- Baseline source is `docs/BENCHMARKS.md` (0.5B Q4_K_M on i7-7700K at ~40 tok/s).
- Infrastructure overhead reference is `docs/operations/PERFORMANCE_BASELINES.md` (~0.02% of 100ms budget).
- CPU model-size scaling uses the existing benchmark guidance table and formula in `docs/BENCHMARKS.md`.
- GPU and multi-GPU throughput uplifts are unvalidated until P2 execution wiring is complete and benchmarked.

### Estimated Throughput by Stage (CPU-First)

| Stage                                 |     0.5B Q4 |     1.5B Q4 |       3B Q4 |         7B Q4 | Confidence           |
| ------------------------------------- | ----------: | ----------: | ----------: | ------------: | -------------------- |
| Current (pre-remediation)             |    40 tok/s |    15 tok/s |     8 tok/s |       4 tok/s | medium               |
| P0 complete                           | 38-42 tok/s | 14-16 tok/s |   7-9 tok/s | 3.5-4.5 tok/s | medium               |
| P1 complete                           | 36-41 tok/s | 13-15 tok/s | 7-8.5 tok/s | 3.3-4.2 tok/s | medium               |
| P2 complete (GPU path active)         | TBD         | TBD         | TBD         | TBD           | unvalidated until empirical benchmark |
| P3 complete (extreme-scale hardening) |         ~P2 |         ~P2 |         ~P2 |           ~P2 | medium               |

Interpretation:

- P0/P1 are primarily correctness and control phases, not raw token-speed phases.
- P2 is the primary throughput uplift phase for large models.
- P3 prioritizes predictability, long-context stability, and tail latency over headline tok/s gains.

### Estimated Memory Envelope by Model Tier (Q4)

| Model Size | Weights | Total Runtime Memory (estimate) | Source Basis         |
| ---------- | ------: | ------------------------------: | -------------------- |
| 0.5B       | 463 MiB |                       ~0.75 GiB | `docs/BENCHMARKS.md` |
| 1.5B       | 1.1 GiB |                        ~1.5 GiB | `docs/BENCHMARKS.md` |
| 3B         | 2.2 GiB |                        ~2.8 GiB | `docs/BENCHMARKS.md` |
| 7B         | 4.5 GiB |                        ~5.5 GiB | `docs/BENCHMARKS.md` |

KV memory model (planning formula): `KV_memory = 2 x layers x hidden_dim x context_len x dtype_size`.
Note: KV cache grows with context length. Validate the model empirically per architecture and precision.

### Phase KPI Gates (Numeric Pass/Fail)

#### P0 Gate: Correctness and Queue Integrity

| KPI                                             | Pass Threshold                                         |
| ----------------------------------------------- | ------------------------------------------------------ |
| Queue drain after burst                         | returns to <=5 pending within 30s after load stops     |
| Queue leak                                      | no monotonic queue growth under steady RPS for 30 min  |
| Queue balance at target load                    | dequeue rate >= enqueue rate at 70% configured throughput |
| Inference admission failures due to `QueueFull` | <1% at 70% of configured max throughput                |
| Cancellation latency                            | p95 <= 250 ms from cancel request to terminal response |
| Runtime availability in stress run              | >= 99.9% over 1h test                                  |

#### P1 Gate: Guardrails and Config Fidelity

| KPI                                       | Pass Threshold                                                  |
| ----------------------------------------- | --------------------------------------------------------------- |
| Config fidelity                           | 100% of declared scale knobs reflected in runtime status output |
| Memory cap enforcement                    | 100% of over-cap requests rejected with typed admission error   |
| Uncontrolled OOM events                   | 0 in 1h stress test                                             |
| p99 latency regression vs P0 at same load | <= 10%                                                          |
| Throughput regression vs P0 at same load  | <= 10%                                                          |

#### P2 Gate: Large-Model Readiness

Prerequisite: model sharding implementation complete for tested mode before applying multi-GPU efficiency gates.

| KPI                                                               | Pass Threshold                                 |
| ----------------------------------------------------------------- | ---------------------------------------------- |
| 7B throughput uplift vs P1 on target GPU hardware                 | >= 3x                                          |
| 13B+ model successful load/infer/unload cycles                    | >= 100 consecutive cycles without leak/failure |
| Multi-GPU scaling efficiency (2 GPUs vs 1 GPU, same model/config) | >= 1.6x throughput                             |
| Multi-GPU scaling efficiency (4 GPUs vs 1 GPU, same model/config) | >= 2.8x throughput                             |
| p99 latency inflation at 80% load vs 50% load                     | <= 1.5x                                        |

#### P3 Gate: Extremely Large Model and Long Context

| KPI                                                   | Pass Threshold                                                  |
| ----------------------------------------------------- | --------------------------------------------------------------- |
| Long-context completion rate (target context profile) | >= 99%                                                          |
| Long-context OOM rate                                 | 0 uncontrolled OOMs in 24h soak                                 |
| Tail latency stability (p99/p50 ratio)                | <= 3.0 at production target load                                |
| Admission reject correctness under pressure           | 100% typed/observable rejection, no silent drops                |
| Canary safety                                         | automatic rollback on threshold breach in <= 2 analysis windows |

### Standard Benchmark Matrix (Run at Every Phase Exit)

| Dimension         | Values                                                     |
| ----------------- | ---------------------------------------------------------- |
| Model tiers       | 0.5B, 1.5B, 3B, 7B, 13B+ (when available)                  |
| Quantization      | Q4 baseline, plus deployment target quant                  |
| Context sizes     | 512, 2048, 4096, long-context target for P3                |
| Concurrency       | 1, 4, 16, 64, saturation point                             |
| Hardware profiles | CPU-only baseline, 1 GPU, 2 GPU, 4 GPU (where available)   |
| Workload mix      | short prompts, long prompts, mixed streaming/non-streaming |

### Metrics to Persist per Run

- Throughput: tok/s and req/s.
- Latency: p50, p95, p99.
- Queue: max depth, average depth, queue wait p95/p99.
- Errors: admission rejections, inference failures, timeout rate.
- Memory: RSS, model memory, KV memory, GPU memory, peak vs steady-state.
- Stability: restart count, crash count, drain-shutdown success.

### Operational Use

- Treat this section as release gates, not advisory guidance.
- Do not promote a phase release unless all phase gates pass on the target environment.
- Store raw benchmark artifacts with build SHA, config snapshot, and hardware manifest.

## Workstream Dependencies

Sequence work using these dependency edges:

- `0.1 Queue-Execution` depends on `0.2 Model Lifecycle` for valid executable handles.
- `0.2 Model Lifecycle` depends on `0.3 API Surface` alignment for FFI/Python parity.
- `1.2 Resource Limits` depends on `0.1 Queue-Execution` as primary admission path.
- `2.3 Smart Loader` depends on `2.1 GPU Path` for backend-backed handles.
- `3.1 Long-Context` depends on `1.1 Config Fidelity` for externally controlled context policy.

## Migration Plan by Release

### Release A (P0)

- Queue-consumer wiring.
- Real model registration/unregistration path.
- FFI/Python API repair.

### Release B (P1)

- Config fidelity and guardrail enforcement.
- CLI/operator completeness for scale diagnostics.

### Release C (P2)

- GPU/multi-GPU execution completion.
- Smart loader and pool integration with real handles.

### Release D (P3)

- Long-context hardening.
- Production-scale Kubernetes profile alignment.

## Risks and Mitigations

- **Risk:** breaking external bindings during API repair.
  - **Mitigation:** ship compatibility wrapper for one release and deprecate with warnings.
- **Risk:** performance regression from strict admission controls.
  - **Mitigation:** benchmark A/B with explicit headroom policies.
- **Risk:** multi-GPU complexity causes unstable behavior on heterogeneous fleets.
  - **Mitigation:** capability matrix tests and explicit unsupported-mode rejection.
- **Risk:** cancellation visibility race with relaxed atomics on cancel flags.
  - **Mitigation:** upgrade cancellation atomic ordering to Acquire/Release or SeqCst and stress-test under contention.
- **Risk:** O(n) model handle lookup introduces lock contention at scale.
  - **Mitigation:** add bidirectional O(1) indexes (`model_id -> handle`, `handle -> model_id`) with invariant tests.
- **Risk:** smart-loader semaphore check can be bypassed under timing races.
  - **Mitigation:** use permit acquisition (`try_acquire`/owned permit) in background load path and add concurrency tests.

## Definition of Done

This remediation path is complete when:

- Runtime request lifecycle is internally consistent and queue-backed.
- Model lifecycle is fully integrated across CLI/IPC/FFI/Python.
- Guardrails are active and observable in production diagnostics.
- Large and extremely large model behaviors are verified under load with reproducible evidence.

---

## Adversarial Review

**Reviewer:** Architect Mode (Automated Code Analysis)
**Date:** 2026-02-22
**Classification:** Critical Architecture Gaps Identified

### Executive Summary of Findings

The remediation document accurately identifies core architectural gaps. However, several claims require nuance, and additional risks not captured in the original document warrant attention. The phased approach is sound, but the document underestimates integration complexity and overestimates the isolation of workstreams.

---

### Claim Verification Matrix

| Claim in Document                      | Code Evidence                                                                                                                                       | Verification  | Notes                                              |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | ------------- | -------------------------------------------------- |
| Runtime enqueues but never dequeues    | [`handler.rs:230-238`](core-runtime/src/ipc/handler.rs:230) enqueues; [`handler.rs:247-281`](core-runtime/src/ipc/handler.rs:247) executes directly | **CONFIRMED** | Queue is tracking-only, not scheduling             |
| Queue has dequeue API, no callsites    | [`queue.rs:148`](core-runtime/src/scheduler/queue.rs:148) defines `dequeue()`                                                                       | **CONFIRMED** | No runtime consumption of queued requests          |
| FFI model load registers metadata only | [`ffi/models.rs:46-51`](core-runtime/src/ffi/models.rs:46) calls `register(metadata, 0)`                                                            | **CONFIRMED** | Memory=0, no inference engine registration         |
| Inference requires registered models   | [`inference.rs:137-141`](core-runtime/src/engine/inference.rs:137) checks `models.get(model_id)`                                                    | **CONFIRMED** | Will fail with `ModelNotLoaded`                    |
| max_context hardcoded to 4096          | [`main.rs:463`](core-runtime/src/main.rs:463), [`lib.rs:101`](core-runtime/src/lib.rs:101)                                                          | **CONFIRMED** | Environment variable `GG_CORE_MAX_CONTEXT` ignored |
| GGUF defaults CPU-first                | [`gguf/mod.rs:38`](core-runtime/src/engine/gguf/mod.rs:38) `n_gpu_layers: 0`                                                                        | **CONFIRMED** | Safe default but requires explicit config for GPU  |
| GPU allocation uses null pointers      | [`gpu.rs:297`](core-runtime/src/engine/gpu.rs:297) `ptr: std::ptr::null_mut()`                                                                      | **CONFIRMED** | Placeholder only, no real allocation               |
| ResourceLimits exists but not wired    | [`limits.rs:57`](core-runtime/src/memory/limits.rs:57) has `try_acquire()`                                                                          | **CONFIRMED** | No callsite in inference hot path                  |
| InferenceParams disables max_memory    | [`inference.rs:78`](core-runtime/src/engine/inference.rs:78) `max_memory_bytes: None`                                                               | **CONFIRMED** | Per-call memory limit bypassed                     |

---

### Critical Gaps Not Adequately Addressed

#### Gap 1: Queue is Not a Queue—It Is a Metrics Sidecar

The document correctly identifies that `dequeue()` is never called, but understates the implication. The current architecture in [`handler.rs:230-281`](core-runtime/src/ipc/handler.rs:230) shows:

```
enqueue() → immediate inference_engine.run() → response
```

The queue serves only to:

1. Track request count for metrics
2. Provide cancellation token storage
3. Reject on saturation (`QueueFull`)

**It does not schedule, prioritize, or batch.** The "scheduler" module exists but is architecturally disconnected from the request path. P0 Workstream 0.1 must not just "implement a consumer" but fundamentally restructure the request flow to make the queue the single execution arbiter.

**Risk:** If P0 only adds a dequeue loop without removing the direct execution path, the system will have dual execution paths with split-brain semantics.

#### Gap 2: Model Registration Bifurcation

The document identifies that FFI loads metadata-only, but does not trace the full implication:

- [`ModelRegistry`](core-runtime/src/models/registry.rs) tracks metadata (name, size, state)
- [`InferenceEngine`](core-runtime/src/engine/inference.rs:93) tracks loaded model instances for execution
- These are **separate data structures** with no synchronization

The FFI path populates `ModelRegistry`. The inference path queries `InferenceEngine.models`. A model can be "loaded" in one but not the other. P0 Workstream 0.2 must create a **unified load pipeline** that atomically updates both registries, or merge them into a single source of truth.

**Risk:** Partial load states where CLI reports model as loaded but inference fails with `ModelNotLoaded`.

#### Gap 3: Smart Loader Returns Placeholder Handles

[`smart_loader.rs:262`](core-runtime/src/models/smart_loader.rs:262) and [`smart_loader.rs:371`](core-runtime/src/models/smart_loader.rs:371) show:

```rust
ModelHandle::new(1) // Placeholder
```

The smart loader's predictive caching and tier-based hints are sophisticated, but the actual load callback is optional and defaults to mmap + touch-first-page. This does not produce a valid inference handle. P2 Workstream 2.3 must not just "integrate" but **replace** the placeholder generation with real model loading that produces handles valid for `InferenceEngine`.

**Risk:** Smart loader reports "Ready" state but inference still fails.

#### Gap 4: GPU Memory Pool Is Allocation-Free

[`gpu.rs:383-400`](core-runtime/src/engine/gpu.rs:383) shows `GpuMemoryPool::allocate()` creates a `GpuMemory` struct with `ptr: std::ptr::null_mut()`. No actual GPU memory is allocated. The pool tracks sizes but does not interact with CUDA/Metal backends.

P2 Workstream 2.1 must implement real allocation, but the document does not address:

- Which backend owns the allocation (CUDA vs Metal vs ROCm)?
- How to handle allocation failures mid-model-load?
- Memory fragmentation over load/unload cycles?

**Risk:** GPU path appears to work in tests (no actual allocation) but fails in production with OOM or corruption.

#### Gap 5: Context Length Check Is Byte-Based, Not Token-Based

[`inference.rs:144`](core-runtime/src/engine/inference.rs:144):

```rust
if prompt.len() > self.max_context_length {
```

This checks **byte length**, not token count. For UTF-8 text, this is incorrect. For tokenized input, it is meaningless. The document mentions "byte-length proxy checks" in P3 Workstream 3.1 but classifies it as an extremely-large-model issue. It is a **correctness issue** that affects all models.

**Risk:** Prompts within token limit but exceeding byte limit are rejected; prompts within byte limit but exceeding token limit cause context overflow during inference.

---

### Underestimated Integration Complexity

#### Workstream Dependencies Not Captured

The document presents workstreams as parallelizable within phases. Code analysis reveals hidden dependencies:

| Workstream          | Hidden Dependency On | Reason                                       |
| ------------------- | -------------------- | -------------------------------------------- |
| 0.1 Queue-Execution | 0.2 Model Lifecycle  | Queue consumer needs valid model handles     |
| 0.2 Model Lifecycle | 0.3 API Surface      | FFI/Python must call new unified load path   |
| 1.2 Resource Limits | 0.1 Queue-Execution  | Admission control must happen at queue entry |
| 2.3 Smart Loader    | 2.1 GPU Path         | Smart loader must produce GPU-backed handles |
| 3.1 Long-Context    | 1.1 Config Fidelity  | Context limits must be configurable first    |

**Recommendation:** Convert phase groups to strict sequences with explicit dependency edges.

#### Test Coverage Gap

The document specifies benchmark tiers but does not require **unit test coverage** for:

- Queue dequeue with cancellation
- Model registry/inference engine synchronization
- Resource limit acquisition/release under contention
- GPU memory pool allocation/deallocation

**Recommendation:** Add mandatory unit test coverage thresholds per workstream.

---

### Performance Estimate Critique

#### GPU Multipliers Are Speculative

The P2 throughput estimates claim "5x-15x" improvement for 7B models on GPU. These are marked "low confidence" but should be marked **unvalidated**. The current codebase has:

- No real GPU allocation
- No multi-GPU execution wiring
- No benchmark harness for GPU paths

The estimates assume optimal CUDA kernel utilization, which is not guaranteed. Real-world GPU speedup depends on:

- Memory bandwidth saturation
- Kernel fusion efficiency
- PCIe/NVLink transfer overhead for multi-GPU

**Recommendation:** Replace multipliers with "TBD after P2 GPU wiring complete" and require empirical measurement before P3.

#### Memory Envelope Missing KV Cache Growth

The memory envelope table lists weights + "total runtime memory" but does not model KV cache growth per context length. For 7B at 4096 context, KV cache can add 1-2 GB depending on precision. P3 mentions this but the baseline table should include KV estimates.

**Recommendation:** Add KV cache memory model to baseline table with formula: `KV_memory = 2 × layers × hidden_dim × context_len × dtype_size`.

---

### KPI Gate Critique

#### P0 Gate: Queue Drain After Burst

> "returns to <=5 pending within 30s after load stops"

This assumes the queue is the bottleneck. If inference is slow (large model, CPU-only), the queue may drain slowly even with correct dequeue semantics. The gate should measure **dequeue rate vs enqueue rate** under steady load, not just post-burst drain.

**Recommendation:** Add KPI: "Dequeue rate >= enqueue rate at 70% max throughput."

#### P2 Gate: Multi-GPU Scaling Efficiency

> ">= 1.6x throughput for 2 GPUs vs 1 GPU"

This is achievable only if:

- Model is sharded across GPUs (pipeline or tensor parallelism)
- P2P bandwidth is sufficient
- Batch size scales with GPU count

The current code has no sharding logic. The gate should be **conditional on sharding implementation**.

**Recommendation:** Add prerequisite: "Model sharding implementation complete" before this gate applies.

---

### Risks Not Captured

#### Risk: Cancellation Token Race

[`queue.rs:72-74`](core-runtime/src/scheduler/queue.rs:72) uses `Ordering::Relaxed` for cancellation flag:

```rust
pub fn is_cancelled(&self) -> bool {
    self.cancelled.load(Ordering::Relaxed)
}
```

Under high contention, a request may be dequeued and start execution after cancellation is requested but before the flag is visible. This is a **correctness race**.

**Recommendation:** Use `Ordering::Acquire/Release` or `SeqCst` for cancellation flags.

#### Risk: Inference Engine Handle Lookup Race

[`inference.rs:197-205`](core-runtime/src/engine/inference.rs:197) iterates to find handle by model_id:

```rust
for (&handle_id, id) in handles.iter() {
    if id == model_id {
        return Some(ModelHandle::new(handle_id));
    }
}
```

This is O(n) and holds a read lock during iteration. Under high model count and concurrent inference, this becomes a bottleneck.

**Recommendation:** Use bidirectional `HashMap<String, ModelHandle>` for O(1) lookup.

#### Risk: Smart Loader Semaphore Bypass

[`smart_loader.rs:233-235`](core-runtime/src/models/smart_loader.rs:233):

```rust
if self.load_semaphore.available_permits() == 0 {
    return; // Another load in progress
}
```

This is a non-blocking check followed by spawn. The permit is never acquired, so multiple background loads can start if timing aligns.

**Recommendation:** Acquire permit in spawned task or use `try_acquire()` with proper handling.

---

### Recommendations Summary

1. **Restructure P0** to make queue the single execution arbiter, removing direct inference path.
2. **Merge or synchronize** `ModelRegistry` and `InferenceEngine.models` in P0.
3. **Add unit test coverage** requirements per workstream.
4. **Replace GPU throughput estimates** with "TBD after wiring" until empirical data available.
5. **Add KV cache memory model** to baseline estimates.
6. **Fix cancellation ordering** to `Acquire/Release` or `SeqCst`.
7. **Add O(1) handle lookup** to inference engine.
8. **Fix smart loader semaphore** to properly acquire permits.
9. **Make context check token-based** in P0, not P3.
10. **Add workstream dependency edges** to prevent parallelization of dependent work.

---

### Conclusion

The remediation document is directionally correct but underestimates the depth of architectural disconnects. The phased approach is appropriate, but workstreams within phases have hidden dependencies that must be sequenced. Several "P2/P3" issues are actually correctness problems that should be addressed in P0. The performance estimates for GPU paths are speculative and should be validated empirically before being used for capacity planning.

**Overall Assessment:** Document requires revision to address hidden dependencies, correctness issues misclassified as scale issues, and speculative performance claims.
