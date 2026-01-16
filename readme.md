# HybridKV

HybridKV is a two-tier key-value system that keeps the authoritative KV engine in user space (full semantics, persistence, complex commands) while offering a kernel-resident hot-key cache for low-latency reads. Unlike single-purpose in-kernel caches, HybridKV frameworkizes the kernel cache mechanism: the kernel provides a general data plane (object storage, indexing, concurrency-optimized read path, invalidation/versioning, memory budgeting, telemetry), and all cache decision logic is implemented as pluggable policies (admission, eviction, hotness estimation, TTL/staleness rules, per-core hotspot replication, and tenant isolation).

This design targets the practical sweet spot: deliver fast read paths for highly skewed (Zipf-like) workloads without pushing full KV semantics into the kernel. HybridKV is suitable for co-located services, sidecars, and latency-sensitive microservices that repeatedly access a small hot set, where kernel caching reduces tail latency and CPU overhead on the critical read path. For deployments that prioritize strict transparency at the network layer, HybridKV can be used alongside a user-space server/proxy; however, the primary focus is a robust kernel hot-cache subsystem with clear operational boundaries.

## Status

**In Development** - Currently in design phase.

## Why HybridKV
### Key strengths

- Frameworkized kernel cache mechanism: a reusable kernel data plane with stable extension points, enabling rapid policy iteration without rewriting the kernel core.

- Read-optimized kernel fast path: concurrency-friendly lookups (e.g., RCU-based indexing) designed for high skew and low tail latency.

- Clear correctness boundary: user space remains the source of truth; kernel cache is an acceleration layer with version-based invalidation and configurable staleness/TTL handling.

- Operational safety by design: hard memory caps, pressure-aware reclamation, fail-open/fallback behavior, and rich telemetry/events for control-loop tuning.

- Multi-tenant readiness: policy-driven budgeting and isolation to reduce cross-tenant interference under shared-cache contention.

- Hotspot scalability options: support for per-core replica policies to mitigate cacheline contention on ultra-hot keys.

### Limitations

- Not a fully transparent “drop-in Redis in-kernel”: HybridKV intentionally does not implement full Redis semantics in kernel; it accelerates only a well-defined hot-read subset.

- Consistency is policy-defined: strict invalidation is supported, but bounded staleness modes require careful configuration and workload validation.

- Kernel complexity and deployment constraints: kernel components demand stricter testing, observability, and operational discipline than pure user-space solutions.

- Best performance requires hot-set stability: workloads with rapidly shifting hot keys or write-heavy patterns may see reduced benefit due to invalidation/promotion churn.

## When to use HybridKV

- HybridKV is a strong fit for:

    - Read-heavy KV workloads with strong skew (hot keys dominate traffic)

    - Low-latency services where p99/p99.9 matters (request routing, feature flags, configs, session tokens, metadata lookups)

    - Co-located architectures (application + KV engine on the same host) where syscall-based fast paths are effective

    - Multi-tenant environments needing explicit cache budgets and isolation policies

- HybridKV is not the best fit for:

    - Write-heavy workloads with frequent updates to hot keys

    - Workloads requiring strict cross-key transactional semantics at the cache layer

    - Environments where kernel deployment is restricted or where a pure user-space cache is operationally preferred

## Architecture

### High-Level Overview

HybridKV is built as a Cargo workspace with six modules:

```
hybridkv/
├── hkv-engine/    # Core storage engine (String, List, Hash, Set, ZSet)
├── hkv-server/    # Network server with hot data tracking
├── hkv-client/    # Client library with smart routing
├── hkv-kernel/    # Kernel module (framework: data plane + policies)
├── hkv-common/    # Shared types and protocols
└── hkv-gui/       # Web-based management dashboard
```

### Framework Architecture: Two-Plane Design

```
┌───────────────────────────────────────────────────────────────────────────┐
│                         HybridKV Framework                                 │
│                                                                            │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │                         KERNEL SPACE                                │  │
│  │  ┌──────────────────────────────────────────────────────────────┐  │  │
│  │  │              KERNEL DATA PLANE (Mechanism)                    │  │  │
│  │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐ │  │  │
│  │  │  │  Object  │  │   RCU    │  │  Memory  │  │ Invalidation │ │  │  │
│  │  │  │ Storage  │  │   Hash   │  │ Governor │  │  & Version   │ │  │  │
│  │  │  │  (Slab)  │  │  Table   │  │ (Limits) │  │   Tracking   │ │  │  │
│  │  │  └──────────┘  └──────────┘  └──────────┘  └──────────────┘ │  │  │
│  │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐ │  │  │
│  │  │  │   Stats  │  │ Char Dev │  │ Netlink  │  │   Safety &   │ │  │  │
│  │  │  │(Counters)│  │/dev/hkv  │  │ Events   │  │   Fallback   │ │  │  │
│  │  │  └──────────┘  └──────────┘  └──────────┘  └──────────────┘ │  │  │
│  │  └────────────────────────┬─────────────────────────────────────┘  │  │
│  │                           │ Policy Interface (Rust Traits)         │  │
│  │  ┌────────────────────────▼─────────────────────────────────────┐  │  │
│  │  │              POLICY PLANE (Pluggable Decisions)              │  │  │
│  │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐ │  │  │
│  │  │  │ Eviction │  │Admission │  │ Hotness  │  │   Tenant     │ │  │  │
│  │  │  │ Policies │  │ Control  │  │Estimator │  │   Budget     │ │  │  │
│  │  │  │          │  │          │  │          │  │  & Fairness  │ │  │  │
│  │  │  │ • LRU    │  │•Threshold│  │• CMS     │  │              │ │  │  │
│  │  │  │ • LFU    │  │• TinyLFU │  │• Sampling│  │• Hard Quota  │ │  │  │
│  │  │  │ • SLRU   │  │• Size-   │  │• Tiered  │  │• Proportional│ │  │  │
│  │  │  │ • TwoQ   │  │  Aware   │  │ Counters │  │• Priority    │ │  │  │
│  │  │  │ • FIFO   │  │          │  │          │  │  Based       │ │  │  │
│  │  │  └──────────┘  └──────────┘  └──────────┘  └──────────────┘ │  │  │
│  │  │  ┌──────────────────────────────────────────────────────────┐ │  │  │
│  │  │  │         TTL & Consistency Policies                       │ │  │  │
│  │  │  │  • Strict Invalidation  • Bounded Staleness              │ │  │  │
│  │  │  │  • Version-Based Check  • Async Refresh                  │ │  │  │
│  │  │  └──────────────────────────────────────────────────────────┘ │  │  │
│  │  └──────────────────────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────────────────────┘  │
│                                    ▲                                      │
│                                    │ ioctl (sync) / Netlink (async)       │
│                                    │                                      │
│  ┌────────────────────────────────┴───────────────────────────────────┐  │
│  │                          USER SPACE                                 │  │
│  │  ┌──────────────────────────────────────────────────────────────┐  │  │
│  │  │  hkv-server (Control Plane)                                   │  │  │
│  │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐ │  │  │
│  │  │  │   RESP   │  │   Hot    │  │Promotion │  │    Admin     │ │  │  │
│  │  │  │ Protocol │  │ Tracking │  │ Manager  │  │     API      │ │  │  │
│  │  │  │  Parser  │  │  (CMS)   │  │(Bg Thread)│  │ (REST/WS)   │ │  │  │
│  │  │  └──────────┘  └──────────┘  └──────────┘  └──────────────┘ │  │  │
│  │  └──────────────────────────────────────────────────────────────┘  │  │
│  │  ┌──────────────────────────────────────────────────────────────┐  │  │
│  │  │  hkv-engine (Storage Engine)                                  │  │  │
│  │  │  • In-memory data structures (String/List/Hash/Set/ZSet)      │  │  │
│  │  │  • Persistence (AOF/RDB)                                       │  │  │
│  │  │  • TTL management                                              │  │  │
│  │  └──────────────────────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────────────────────┘  │
│                                    ▲                                      │
│                                    │ Client Library (hkv-client)          │
│                                    │                                      │
│  ┌────────────────────────────────┴───────────────────────────────────┐  │
│  │                      Client Application                             │  │
│  │  ┌──────────────┐      Fast Path: Kernel cache hit (2-10μs)        │  │
│  │  │  Try Kernel  ├─────► Slow Path: User-space miss (20-40μs)       │  │
│  │  │   First?     │      Fallback: On kernel error                    │  │
│  │  └──────────────┘                                                    │  │
│  └────────────────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────────────────┘
```

### Request Flow

**Read Path (Fast Path)**:
```
1. Client → hkv-client.get("key")
2. hkv-client → ioctl(CMD_READ, "key") to /dev/hybridkv
3. Kernel → RCU hash lookup (lock-free, 20-50ns)
4. If HIT:
   → copy_to_user (value)
   → Policy: update LRU/access counters
   → Return to client (total: 2-10μs)
5. If MISS:
   → Fallback to user-space
   → hkv-client → TCP/RESP to hkv-server
   → hkv-server → hkv-engine.get("key")
   → Return to client (total: 20-40μs)
```

**Write Path (Write-Through)**:
```
1. Client → hkv-client.set("key", "value")
2. hkv-client → TCP/RESP to hkv-server (always user-space)
3. hkv-server → hkv-engine.set("key", "value")
4. hkv-server → ioctl(CMD_INVALIDATE, "key") to kernel
5. Kernel → Mark entry stale or remove (consistency mode)
6. Background: Promotion Manager decides if re-promotion needed
```

**Promotion Flow (Background)**:
```
1. Hot Tracker → Count-Min Sketch tracks all accesses
2. Promotion Manager (5-sec interval):
   → Analyze hot set (frequency > 100 QPS, read ratio > 90%)
   → Policy: Admission control decides what to promote
   → ioctl(CMD_BATCH_PROMOTE, keys[]) to kernel
3. Kernel → Policy: Eviction if memory pressure
   → Insert into RCU hash table
   → Netlink notify user-space of evictions
```

**Eviction Flow (Autonomous)**:
```
1. Kernel: Memory usage > 80% watermark
2. Policy: EvictionPolicy.select_victims(count=100)
   → LRU: Pick least recently used
   → LFU: Pick least frequently used
   → SLRU: Evict from probation list first
3. Kernel: Remove entries, free memory
4. Kernel → Netlink: Notify user-space of evicted keys
5. User-space: Update hot tracking state
```

### Multi-Tenant Isolation

```
┌────────────────────────────────────────────────────────┐
│              Kernel Cache (256MB Total)                 │
├────────────────────────────────────────────────────────┤
│  Tenant A (100MB quota)    │  Tenant B (100MB quota)   │
│  ┌──────────────────────┐  │  ┌──────────────────────┐│
│  │ 80MB used            │  │  │ 60MB used            ││
│  │ 40K entries          │  │  │ 30K entries          ││
│  │ Hit rate: 85%        │  │  │ Hit rate: 75%        ││
│  └──────────────────────┘  │  └──────────────────────┘│
│                            │                           │
│  Shared Pool (56MB available for proportional sharing) │
│  ┌───────────────────────────────────────────────────┐ │
│  │ Allocated based on weight & min guarantees        │ │
│  └───────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────┘

Isolation Features:
• Hard quotas prevent one tenant from starving others
• Independent eviction domains (tenant A eviction ≠ tenant B)
• Per-tenant statistics and monitoring
• Priority-based allocation (production > batch workloads)
```

**How It Works**:
1. All writes go to user-space storage engine (hkv-engine) - authoritative
2. Background thread tracks hot keys and promotes to kernel cache
3. Reads check kernel cache first (fast path), fall back to user space
4. Kernel autonomously evicts cold keys using configured policy (LRU/LFU/etc.)
5. Multi-tenant policies ensure fairness and isolation under contention


## Documentation

Still working...



