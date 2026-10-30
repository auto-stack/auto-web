pub use auto_lang_core::api::TargetGenerator;
mod axum;
mod typescript;
pub use axum::AxumGenerator;
pub use typescript::TypeScriptGenerator;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Axum,
    TypeScript,
}
impl Target {
    pub fn generator(&self) -> Box<dyn TargetGenerator> {
        match self {
            Self::Axum => Box::new(AxumGenerator::new()),
            Self::TypeScript => Box::new(TypeScriptGenerator::new()),
        }
    }
}
#[cfg(test)]
mod integration_tests;
