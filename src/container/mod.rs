//! Container management
/// This crate is responsible for the creation of containers. It provides a builder that can
/// be used to configure and create containers. We distinguish between an init container for which
/// namespaces and cgroups will be created (usually) and a tenant container process that will move
/// into the existing namespaces and cgroups of the initial container process (e.g. used to implement
/// the exec command).
pub mod builder;
mod builder_impl;
#[allow(clippy::module_inception)]
mod container;
pub mod container_delete;
pub mod container_events;
pub mod container_kill;
pub mod container_pause;
pub mod container_resume;
pub mod container_start;
pub mod init_builder;
pub mod state;
pub mod tenant_builder;
pub use container::Container;
pub use state::{ContainerStatus, State};
