pub mod common;
pub mod config;
pub mod error;
pub mod service;
pub mod idgen {
    tonic::include_proto!("idgen");
}
pub mod telemetry;
