# Quickstart: 10 Minutes to First Inference

**Target Audience:** DevOps Engineers, SREs, ML Engineers  
**Prerequisites:** Basic Kubernetes knowledge  
**Time Required:** ~10 minutes

---

## Overview

This guide will get you from zero to running your first LLM inference in approximately 10 minutes. By the end, you'll have:

- A running Veritas SPARK deployment
- A loaded model ready for inference
- A successful test inference

---

## Prerequisites Check (2 minutes)

### Required Tools

| Tool       | Version | Check Command              |
| ---------- | ------- | -------------------------- |
| Kubernetes | 1.28+   | `kubectl version --short`  |
| Helm       | 3.12+   | `helm version --short`     |
| kubectl    | Latest  | `kubectl version --client` |

### Required Resources

| Resource  | Minimum  | Recommended      |
| --------- | -------- | ---------------- |
| CPU Cores | 4        | 8+               |
| Memory    | 16GB     | 32GB+            |
| GPU       | Optional | NVIDIA A100/H100 |
| Storage   | 50GB     | 100GB+ SSD       |

### Quick Prerequisite Check

```bash
# Check Kubernetes connection
kubectl cluster-info

# Check node resources
kubectl describe nodes | grep -A 5 "Allocated resources"

# Check GPU availability (if applicable)
kubectl describe nodes | grep nvidia.com/gpu
```

---

## Step 1: Add Helm Repository (1 minute)

```bash
# Add the Veritas SPARK Helm repository
helm repo add veritas-spark https://charts.veritas-spark.io

# Update repository
helm repo update

# Verify chart is available
helm search repo veritas-spark
```

**Expected Output:**

```
NAME                    CHART VERSION   APP VERSION     DESCRIPTION
veritas-spark/veritas-spark 0.6.0           0.6.0           Secure LLM Inference Runtime
```

---

## Step 2: Create Namespace (30 seconds)

```bash
# Create dedicated namespace
kubectl create namespace veritas-spark

# Set as default for subsequent commands
kubectl config set-context --current --namespace=veritas-spark
```

---

## Step 3: Deploy Veritas SPARK (3 minutes)

### Option A: Quick Development Deployment (CPU-only)

```bash
# Deploy with minimal configuration for evaluation
helm install veritas-spark veritas-spark/veritas-spark \
  --namespace veritas-spark \
  --set replicaCount=1 \
  --set resources.limits.cpu=2 \
  --set resources.limits.memory=4Gi \
  --set model.enabled=true \
  --set model.name="llama-2-7b-chat" \
  --set model.quantization="q4_0"
```

### Option B: Single GPU Deployment

```bash
# Deploy with GPU support
helm install veritas-spark veritas-spark/veritas-spark \
  --namespace veritas-spark \
  --set replicaCount=1 \
  --set resources.limits.nvidia.com/gpu=1 \
  --set model.enabled=true \
  --set model.name="llama-2-7b-chat"
```

### Option C: Using Example Values File

```bash
# Download example values file
curl -O https://raw.githubusercontent.com/veritas-spark/charts/main/examples/values-dev.yaml

# Deploy with example values
helm install veritas-spark veritas-spark/veritas-spark \
  --namespace veritas-spark \
  -f values-dev.yaml
```

---

## Step 4: Verify Deployment (2 minutes)

### Check Pod Status

```bash
# Watch pods start
kubectl get pods -w

# Wait for pod to be ready (Ctrl+C when ready)
```

**Expected Output:**

```
NAME                           READY   STATUS    RESTARTS   AGE
veritas-spark-6f8b9c4d-xyz12     0/1     Pending   0          0s
veritas-spark-6f8b9c4d-xyz12     0/1     ContainerCreating   0   2s
veritas-spark-6f8b9c4d-xyz12     1/1     Running   0          45s
```

### Check Deployment Health

```bash
# Check deployment status
kubectl get deployment veritas-spark

# Check service
kubectl get svc veritas-spark

# Check model loading
kubectl logs -l app.kubernetes.io/name=veritas-spark --tail=50 | grep -i model
```

### Run Verification Command

```bash
# If veritas-spark CLI is installed
veritas-spark verify

# Or use kubectl
kubectl exec -it deployment/veritas-spark -- veritas-spark-verify
```

**Expected Output:**

```
✓ Pod is running
✓ Service is accessible
✓ Model loaded: llama-2-7b-chat
✓ Health check passed
✓ Ready for inference
```

---

## Step 5: Run First Inference (2 minutes)

### Port Forward (if not using LoadBalancer/Ingress)

```bash
# Forward local port to service
kubectl port-forward svc/veritas-spark 8080:8080 &

# Wait for port forward to establish
sleep 2
```

### Test Inference

```bash
# Simple completion request
curl -X POST http://localhost:8080/v1/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama-2-7b-chat",
    "prompt": "Explain quantum computing in one sentence.",
    "max_tokens": 50,
    "temperature": 0.7
  }'
```

**Expected Response:**

```json
{
  "id": "cmpl-abc123",
  "object": "text_completion",
  "created": 1708123456,
  "model": "llama-2-7b-chat",
  "choices": [
    {
      "text": "Quantum computing uses quantum mechanical phenomena like superposition and entanglement to process information in ways that classical computers cannot, potentially solving certain problems exponentially faster.",
      "index": 0,
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 8,
    "completion_tokens": 32,
    "total_tokens": 40
  }
}
```

### Streaming Inference

```bash
# Streaming completion request
curl -X POST http://localhost:8080/v1/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama-2-7b-chat",
    "prompt": "Write a haiku about AI:",
    "max_tokens": 30,
    "stream": true
  }'
```

---

## Step 6: Explore Further (Optional)

### Check Available Models

```bash
# List loaded models
curl http://localhost:8080/v1/models

# Or via kubectl
kubectl exec -it deployment/veritas-spark -- veritas-spark models list
```

### View Metrics

```bash
# Prometheus metrics endpoint
curl http://localhost:8080/metrics

# Key metrics to check
curl http://localhost:8080/metrics | grep -E "veritas_(requests|latency|tokens)"
```

### Check Logs

```bash
# View recent logs
kubectl logs -l app.kubernetes.io/name=veritas-spark --tail=100

# Follow logs
kubectl logs -f -l app.kubernetes.io/name=veritas-spark
```

---

## Troubleshooting

### Pod Stuck in Pending

```bash
# Check events
kubectl describe pod -l app.kubernetes.io/name=veritas-spark

# Common causes:
# - Insufficient resources: Reduce resource requests
# - GPU not available: Use CPU-only deployment
# - PVC not binding: Check storage class
```

### Model Not Loading

```bash
# Check model loading logs
kubectl logs -l app.kubernetes.io/name=veritas-spark | grep -i "model\|error"

# Common causes:
# - Insufficient memory: Use quantized model
# - Model not found: Check model name
# - Download timeout: Check network connectivity
```

### Connection Refused

```bash
# Check service
kubectl get svc veritas-spark
kubectl describe svc veritas-spark

# Check endpoints
kubectl get endpoints veritas-spark

# Verify port forward is running
ps aux | grep port-forward
```

### GPU Not Detected

```bash
# Check GPU resources
kubectl describe nodes | grep -A 10 "nvidia.com/gpu"

# Check GPU operator (if using NVIDIA)
kubectl get pods -n gpu-operator

# Verify GPU driver
kubectl exec -it deployment/veritas-spark -- nvidia-smi
```

---

## Cleanup

```bash
# Uninstall Helm release
helm uninstall veritas-spark --namespace veritas-spark

# Delete namespace
kubectl delete namespace veritas-spark

# Remove Helm repository (optional)
helm repo remove veritas-spark
```

---

## Next Steps

1. **Configuration:** See [values.yaml examples](../../k8s/helm/veritas-spark/examples/) for production configurations
2. **Deployment Strategies:** Read [ADR-006](../architecture/ADR-006-DEPLOYMENT-STRATEGIES.md) for canary/blue-green deployments
3. **Security:** Review [Security Posture Baseline](../security/SECURITY_POSTURE_BASELINE.md) for production hardening
4. **Operations:** Check [Deployment Troubleshooting](./DEPLOYMENT_TROUBLESHOOTING.md) for common issues
5. **Monitoring:** Import Grafana dashboards for observability

---

## Getting Help

| Resource           | Link                                       |
| ------------------ | ------------------------------------------ |
| Documentation      | https://docs.veritas-spark.io                |
| GitHub Issues      | https://github.com/veritas-spark/core/issues |
| Community Slack    | https://slack.veritas-spark.io               |
| Enterprise Support | support@veritas-spark.io                     |

---

## Quick Reference

### Essential Commands

```bash
# Install
helm install veritas-spark veritas-spark/veritas-spark -n veritas-spark

# Upgrade
helm upgrade veritas-spark veritas-spark/veritas-spark -n veritas-spark

# Rollback
helm rollback veritas-spark -n veritas-spark

# Uninstall
helm uninstall veritas-spark -n veritas-spark

# Check status
kubectl get all -n veritas-spark

# View logs
kubectl logs -f -l app.kubernetes.io/name=veritas-spark -n veritas-spark

# Port forward
kubectl port-forward svc/veritas-spark 8080:8080 -n veritas-spark

# Test inference
curl http://localhost:8080/v1/models
```

### Example values.yaml

```yaml
# Minimal development configuration
replicaCount: 1

resources:
  limits:
    cpu: 2
    memory: 4Gi
  requests:
    cpu: 1
    memory: 2Gi

model:
  enabled: true
  name: "llama-2-7b-chat"
  quantization: "q4_0"

service:
  type: ClusterIP
  port: 8080

# Disable GPU for CPU-only
gpu:
  enabled: false
```

---

**Congratulations!** You've successfully deployed Veritas SPARK and run your first inference. Welcome to secure, production-ready LLM inference!
