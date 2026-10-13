#[cfg(target_arch = "wasm32")]
mod http;
#[cfg(any(test, target_arch = "wasm32"))]
mod model;
#[cfg(target_arch = "wasm32")]
mod plugin;
