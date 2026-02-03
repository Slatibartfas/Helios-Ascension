---
description: 'Performance optimization guidelines for Helios Ascension'
applyTo: '**/*.rs'
---

# Performance Optimization Guidelines

Optimize Helios Ascension for high performance while maintaining code clarity.

## Performance Philosophy

- Profile before optimizing - measure, don't guess
- Focus on algorithmic improvements first
- Optimize hot paths identified by profiling
- Keep code readable while improving performance
- Use Bevy's built-in optimization features

## Profiling Tools

- Use `cargo flamegraph` for CPU profiling
- Use Bevy's diagnostic plugins for frame timing
- Monitor entity counts and system execution time
- Profile both debug and release builds
- Focus on gameplay-critical systems first

## Bevy Performance Patterns

### Query Optimization
- Use query filters (`With<T>`, `Without<T>`) to reduce iteration
- Use change detection queries (`Changed<T>`) to skip unchanged entities
- Avoid querying all entities when possible
- Use marker components for entity categorization

### System Optimization
- Systems run in parallel by default - leverage this
- Use run conditions to skip unnecessary system execution
- Order systems to maximize parallelism
- Avoid mutable resource access to enable parallel execution

### Entity Management
- Minimize entity spawning/despawning in hot loops
- Use entity recycling for frequently created/destroyed entities
- Use sparse sets for components with few entities
- Bundle components together for better cache locality

## Memory Optimization

- Avoid unnecessary allocations
- Reuse buffers and collections
- Use stack allocation when possible
- Prefer borrowing over cloning
- Use `Vec::with_capacity()` when size is known

## Iteration Patterns

- Use iterators instead of index-based loops
- Prefer lazy iterators over eager collection
- Use `iter()` over `into_iter()` when ownership not needed
- Chain iterator operations for efficiency
- Avoid intermediate collections

## Compilation Optimization

- Use release profiles for performance testing
- Consider the `fast` profile for quick iteration
- Use LTO (Link Time Optimization) for release builds
- Enable CPU-specific optimizations when appropriate
- Profile different optimization levels

## Data Structure Choices

- Use `Vec` for contiguous data access
- Use `HashMap` for key-based lookup
- Consider specialized collections (e.g., `SmallVec`)
- Use fixed-size arrays when size is known
- Prefer flat data structures over deep nesting

## Hot Path Optimization

- Identify hot paths through profiling
- Minimize allocations in hot paths
- Cache frequently computed values
- Use SIMD when beneficial (via `glam`)
- Consider manual loop unrolling for critical loops

## Best Practices

- Measure performance impact of changes
- Optimize for the common case
- Don't prematurely optimize
- Keep optimizations local and documented
- Balance performance with maintainability
- Test performance on target hardware
- Monitor frame times and system budgets
