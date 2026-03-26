//! Generated gRPC stubs for LogisticOS inter-service communication.
//! Build with `cargo build` — tonic-build generates these from .proto files.

pub mod identity {
    tonic::include_proto!("logisticos.identity.v1");
}
pub mod order {
    tonic::include_proto!("logisticos.order.v1");
}
pub mod dispatch {
    tonic::include_proto!("logisticos.dispatch.v1");
}
pub mod driver {
    tonic::include_proto!("logisticos.driver.v1");
}
pub mod cdp {
    tonic::include_proto!("logisticos.cdp.v1");
}
pub mod engagement {
    tonic::include_proto!("logisticos.engagement.v1");
}
pub mod payments {
    tonic::include_proto!("logisticos.payments.v1");
}
pub mod analytics {
    tonic::include_proto!("logisticos.analytics.v1");
}
pub mod pod {
    tonic::include_proto!("logisticos.pod.v1");
}
pub mod fleet {
    tonic::include_proto!("logisticos.fleet.v1");
}
pub mod carrier {
    tonic::include_proto!("logisticos.carrier.v1");
}
