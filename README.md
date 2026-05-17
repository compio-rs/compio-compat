# Archive notice

This crate has been merged into the main stream of [`compio`](https://github.com/compio-rs/compio).

# compio-compat

Run compio in other async runtimes.

## Usage
```rust
use compio_compat::{RuntimeCompat, TokioAdapter};

#[tokio::main]
async fn main() {
    // Create a compio runtime:
    let runtime = compio::runtime::Runtime::new().unwrap();
    // Create the compat layer:
    let runtime = RuntimeCompat::<TokioAdapter>::new(runtime).unwrap();
    // Execute your future:
    runtime.execute(async {
        // Run compio-specific code
    }).await;
}
```
