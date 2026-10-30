//! HTTP domain runtime; explicitly composed with the language SDK.
pub use auto_web_contract as contract;
extern crate self as auto_web;
pub use auto_lang_core::{error, AutoResult};
pub mod api {
    pub use crate::api_targets::{AxumGenerator, Target, TypeScriptGenerator};
    pub use auto_lang_core::api::*;
    pub mod targets {
        pub use crate::api_targets::{AxumGenerator, Target, TypeScriptGenerator};
        pub use auto_lang_core::api::targets::*;
    }
}
pub mod api_gen;
mod api_targets;
pub use api_gen::{backend_generation_is_fresh, generate_api};
pub use auto_generation_core::{shared_generation, sidecar, source_owner};
pub mod rust_workspace;

pub mod http_file_service;
pub mod http_server;
pub mod http_service_config;
pub mod http_service_observability;
pub mod http_transport;
pub mod http_upload_service;
pub mod vm {
    pub use auto_lang_core::vm::*;
    pub mod ffi {
        pub use crate::{http_server, http_transport};
        pub use auto_lang_core::vm::ffi::*;
    }
}
pub use auto_lang_core::vm::ffi::{
    async_http, axum_adapter, http_return_resource, http_server_file, http_stream, http_transfer,
    http_upload, stdlib,
};
pub use http_service_config::ServiceRuntime;
pub fn service_launch(
    config: std::sync::Arc<contract::HttpServiceConfig>,
) -> vm::host_capabilities::http_service::HttpServiceLaunch {
    vm::host_capabilities::http_service::HttpServiceLaunch::new(config)
}

mod host;
pub use host::{
    install_http_service_host_if_absent, listen_borrowed, serve_owned, WebHttpServiceHost,
};

#[cfg(all(test, feature = "test-http-e2e"))]
fn run(code: &str) -> auto_lang_core::AutoResult<String> {
    install_http_service_host_if_absent();
    auto_lang_core::run(code)
}
#[cfg(all(test, feature = "test-http-e2e"))]
fn run_with_capture_and_path(
    code: &str,
    path: &str,
) -> auto_lang_core::AutoResult<(String, String)> {
    install_http_service_host_if_absent();
    auto_lang_core::run_with_capture_and_path(code, path)
}
#[cfg(test)]
use auto_lang_core::block_on_autovm_local;
