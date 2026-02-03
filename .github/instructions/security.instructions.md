---
description: 'Security best practices for Helios Ascension'
applyTo: '**/*.rs'
---

# Security Best Practices

Follow these security guidelines to ensure Helios Ascension is safe and robust.

## Memory Safety

- Leverage Rust's ownership system for automatic memory safety
- Avoid `unsafe` code unless absolutely necessary
- Document all `unsafe` blocks with safety rationale
- Use safe abstractions over unsafe code
- Audit all uses of `unsafe` carefully

## Input Validation

- Validate all user inputs before processing
- Check bounds for array and vector access
- Validate file paths before file operations
- Sanitize data before deserialization
- Return errors for invalid inputs rather than panicking

## Deserialization Security

- Be cautious when deserializing data from untrusted sources
- Use `serde`'s validation features
- Set reasonable limits on deserialized data sizes
- Validate deserialized data structure
- Consider using checksums for data integrity

## Dependency Management

- Keep dependencies up to date
- Review dependencies for security issues
- Use `cargo audit` to check for known vulnerabilities
- Minimize dependency count
- Prefer well-maintained crates

## Error Handling

- Don't expose sensitive information in error messages
- Handle errors gracefully without crashing
- Use proper error types with context
- Log errors appropriately
- Don't panic on user input errors

## Resource Management

- Prevent resource exhaustion attacks
- Set limits on entity counts and data sizes
- Monitor memory usage
- Clean up resources properly
- Handle out-of-memory conditions gracefully

## Best Practices

- Use Rust's type system for safety guarantees
- Prefer immutable data structures
- Use standard library security features
- Follow principle of least privilege
- Keep security considerations in code reviews
