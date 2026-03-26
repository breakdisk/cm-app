pub mod envelope;
pub mod producer;
pub mod consumer;
pub mod topics;
pub mod payloads;

// Re-export the main types at crate root
pub use envelope::Event;
pub use producer::KafkaProducer;
