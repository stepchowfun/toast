# Contributing

Thank you for your interest in contributing! You can contribute by filing [issues](https://github.com/stepchowfun/toast/issues) and submitting [pull requests](https://github.com/stepchowfun/toast/pulls). Please observe our [code of conduct](https://github.com/stepchowfun/toast/blob/main/CODE_OF_CONDUCT.md).

If you submit a pull request, please ensure your change passes the [GitHub Actions](https://github.com/stepchowfun/toast/actions) CI checks. This will be apparent from the required status check(s) in the pull request.

## Rust style guide

We are fortunate to have good tooling around enforcing a consistent style throughout the codebase. If you have [Toast](https://github.com/stepchowfun/toast), you can run the various lint checks by running `toast lint`. Otherwise, you can rely on our CI to do it for you. Here, we make note of a few conventions which are not yet enforced automatically. Please adhere to these conventions when possible, and provide appropriate justification for deviations from this guide. If you notice any style violations which appear unintentional, we invite you to bring them to our attention.

### Comments

**Rule:** Comments should be written in American English.

**Rule:** Comments should always be capitalized unless they start with a code-like expression (see below).

**Rule:** Comments which are sentences should be punctuated appropriately. For example:

```rust
// The following logic implements beta reduction.
```

**Rule:** Comments which are not sentences should not have a trailing period. For example:

```rust
// Already normalized
```

**Rule:** Code-like expressions, such as variable names, should be surrounded by backticks. For example:

```rust
// `source_range` is a half-open interval, closed on the left and open on the right.
```

### Trailing commas

The linter enforces that items in multi-line sequences (e.g., function arguments and macro arguments) have trailing commas.

**Rule:** Macros should be written to accept trailing commas as follows:

```rust
macro_rules! my_macro {
    ($foo:expr, $bar:expr, $baz:expr $(,)?) => {{
        ...
    }};
}
```
