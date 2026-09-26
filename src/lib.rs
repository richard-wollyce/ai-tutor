//! Biblioteca Wollyce: AI Tutor local-first com memória determinística Ulpia,
//! esteira de ingestão restrita a .pdf, .txt e .md, defesa contra prompt injection
//! e contabilidade de consumo de tokens em SQLite.

pub mod security;
pub mod metering;
pub mod ingest;
pub mod tutor;
pub mod server;
pub mod storage;
pub mod engine;
