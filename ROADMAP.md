# Veritas SDR Feature Roadmap

**Last Updated:** 2026-02-16  
**Version:** 0.2.0

---

## Current Status: Pre-Production

Veritas SDR is in pre-production status. Core functionality is complete and tested, but production deployment requires additional work.

---

## Completed Features (v0.1.0)

### Core Runtime

- [x] IPC-based communication (no HTTP overhead)
- [x] Dual backend support (GGUF + ONNX)
- [x] Memory management with arena allocation
- [x] Work-stealing scheduler
- [x] Streaming output support

### Security

- [x] Prompt injection detection (55+ patterns)
- [x] PII detection (13 types)
- [x] Output sanitization
- [x] Model encryption (AES-256)
- [x] Sandbox isolation (Windows Job Objects)
- [x] Rate limiting
- [x] Audit logging

### Performance

- [x] KV Cache with paged attention
- [x] Speculative decoding v2
- [x] SIMD tokenizer v2
- [x] Thread pool tuning

### Testing

- [x] Tier 1: Unit tests (passing)
- [x] Tier 2: Integration tests (37/37 passing)
- [x] Tier 3: Optimization tests (30/30 passing)
- [x] Security tests (43/43 passing)

### Documentation

- [x] README with transparent claims
- [x] Usage guide
- [x] Comparative analysis
- [x] Honest assessment document
- [x] Rust enterprise analysis
- [x] Dependency analysis

---

## Completed Features (v0.2.0)

### GPU Support

- [x] CUDA backend implementation (cudarc bindings)
- [x] Metal backend (macOS with metal crate)
- [x] GPU memory management with actual allocations
- [x] Multi-GPU support (layer/tensor/pipeline parallelism)
- [x] Flash attention GPU kernels

**Status:** Framework implemented with CUDA/Metal bindings

### GPU Architecture

- [`GpuManager`](core-runtime/src/engine/gpu.rs) - Device detection and memory management
- [`CudaBackend`](core-runtime/src/engine/cuda.rs) - NVIDIA CUDA support via cudarc
- [`MetalBackend`](core-runtime/src/engine/metal.rs) - Apple Metal support (macOS)
- [`MultiGpuManager`](core-runtime/src/engine/multi_gpu.rs) - Multi-GPU coordination
- [`FlashAttnGpu`](core-runtime/src/engine/flash_attn_gpu.rs) - Memory-efficient attention

### GPU Features

- Device detection and capability querying
- Memory pool allocation
- Compute capability checks (CUDA 8.0+ for flash attention)
- Unified memory support (Apple Silicon)
- P2P communication detection
- Automatic strategy selection for multi-GPU

---

## In Progress (v0.2.1)

### Benchmarking

- [ ] llama.cpp direct comparison
- [ ] Security overhead measurement
- [ ] GPU vs CPU comparison
- [ ] Memory profiling

**Status:** Benchmark scripts created, needs execution

---

## Planned Features (v0.3.0)

### Performance

- [ ] Distributed inference
- [ ] Model parallelism
- [ ] Pipeline parallelism
- [ ] Continuous batching improvements

### Model Support

- [ ] Mixture of Experts (MoE) optimization
- [ ] Multi-modal support
- [ ] Custom model architectures

### API

- [ ] C API for non-Rust integration
- [ ] Python bindings
- [ ] REST API (optional, for compatibility)

---

## Planned Features (v0.4.0)

### Enterprise

- [ ] Kubernetes operator
- [ ] Prometheus metrics
- [ ] OpenTelemetry integration
- [ ] Health check endpoints

### Security

- [ ] Independent security audit
- [ ] SOC 2 compliance preparation
- [ ] FIPS 140-2 consideration

### Operations

- [ ] Model registry
- [ ] A/B testing support
- [ ] Canary deployments

---

## Future Considerations

### Research

- [ ] Custom quantization methods
- [ ] Novel attention mechanisms
- [ ] Hardware-specific optimizations

### Ecosystem

- [ ] Plugin system
- [ ] Community model zoo
- [ ] Integration with ML frameworks

---

## Release Timeline

| Version   | Target  | Focus                              |
| --------- | ------- | ---------------------------------- |
| **0.1.0** | Current | Core functionality, security       |
| **0.2.0** | Q2 2026 | GPU support, benchmarking          |
| **0.3.0** | Q3 2026 | Distributed inference, multi-modal |
| **0.4.0** | Q4 2026 | Enterprise features, compliance    |
| **1.0.0** | 2027    | Production stable release          |

---

## Contributing

We welcome contributions! See [CLA.md](CLA.md) for contributor license agreement.

### Priority Areas

1. **GPU Support** - CUDA/Metal implementation
2. **Benchmarking** - Fair performance comparisons
3. **Documentation** - Examples, tutorials
4. **Testing** - Edge cases, stress tests

---

## Feedback

- **Issues:** GitHub Issues
- **Security:** See [SECURITY.md](SECURITY.md)
- **Discussions:** GitHub Discussions

---

Copyright 2024-2026 Veritas SDR Contributors  
Licensed under the Apache License, Version 2.0
