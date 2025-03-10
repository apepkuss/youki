use anyhow::{bail, Context, Result};
use nix::unistd;
use oci_spec::runtime::Spec;
use rootless::Rootless;
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    apparmor, config::YoukiConfig, notify_socket::NOTIFY_FILE, process::args::ContainerType,
    rootless, tty, utils,
};

use super::{
    builder::ContainerBuilder, builder_impl::ContainerBuilderImpl, Container, ContainerStatus,
};

// Builder that can be used to configure the properties of a new container
pub struct InitContainerBuilder<'a> {
    base: ContainerBuilder<'a>,
    bundle: PathBuf,
    use_systemd: bool,
}

impl<'a> InitContainerBuilder<'a> {
    /// Generates the base configuration for a new container from which
    /// configuration methods can be chained
    pub(super) fn new(builder: ContainerBuilder<'a>, bundle: PathBuf) -> Self {
        Self {
            base: builder,
            bundle,
            use_systemd: true,
        }
    }

    /// Sets if systemd should be used for managing cgroups
    pub fn with_systemd(mut self, should_use: bool) -> Self {
        self.use_systemd = should_use;
        self
    }

    /// Creates a new container
    pub fn build(self) -> Result<Container> {
        let spec = self.load_spec().context("failed to load spec")?;
        let container_dir = self
            .create_container_dir()
            .context("failed to create container dir")?;

        let mut container = self
            .create_container_state(&container_dir)
            .context("failed to create container state")?;
        container
            .set_systemd(self.use_systemd)
            .set_annotations(spec.annotations().clone());

        unistd::chdir(&container_dir)?;
        let notify_path = container_dir.join(NOTIFY_FILE);
        // convert path of root file system of the container to absolute path
        let rootfs = fs::canonicalize(spec.root().as_ref().context("no root in spec")?.path())?;

        // if socket file path is given in commandline options,
        // get file descriptors of console socket
        let csocketfd = if let Some(console_socket) = &self.base.console_socket {
            Some(tty::setup_console_socket(
                &container_dir,
                console_socket,
                "console-socket",
            )?)
        } else {
            None
        };

        let rootless = Rootless::new(&spec)?;
        let config = YoukiConfig::from_spec(&spec, container.id(), rootless.is_some())?;
        config
            .save(&container_dir)
            .context("failed to save config")?;

        let mut builder_impl = ContainerBuilderImpl {
            container_type: ContainerType::InitContainer,
            syscall: self.base.syscall,
            container_id: self.base.container_id,
            pid_file: self.base.pid_file,
            console_socket: csocketfd,
            use_systemd: self.use_systemd,
            spec: &spec,
            rootfs,
            rootless,
            notify_path,
            container: Some(container.clone()),
            preserve_fds: self.base.preserve_fds,
            detached: false, // TODO this should be set properly based on how the command is given
        };

        builder_impl.create()?;
        container.refresh_state()?;

        Ok(container)
    }

    fn create_container_dir(&self) -> Result<PathBuf> {
        let container_dir = self.base.root_path.join(&self.base.container_id);
        log::debug!("container directory will be {:?}", container_dir);

        if container_dir.exists() {
            bail!("container {} already exists", self.base.container_id);
        }

        utils::create_dir_all(&container_dir).context("failed to create container dir")?;

        Ok(container_dir)
    }

    fn load_spec(&self) -> Result<Spec> {
        let source_spec_path = self.bundle.join("config.json");
        let mut spec = Spec::load(&source_spec_path)?;
        Self::validate_spec(&spec).context("failed to validate runtime spec")?;

        spec.canonicalize_rootfs(&self.bundle)
            .context("failed to canonicalize rootfs")?;
        Ok(spec)
    }

    fn validate_spec(spec: &Spec) -> Result<()> {
        if !spec.version().starts_with("1.0") {
            bail!(
                "runtime spec has incompatible version '{}'. Only 1.0.X is supported",
                spec.version()
            );
        }

        if let Some(process) = spec.process() {
            if let Some(profile) = process.apparmor_profile() {
                if !apparmor::is_enabled()? {
                    bail!(
                        "apparmor profile {} is specified in runtime spec, \
                    but apparmor is not activated on this system",
                        profile
                    );
                }
            }
        }

        Ok(())
    }

    fn create_container_state(&self, container_dir: &Path) -> Result<Container> {
        let container = Container::new(
            &self.base.container_id,
            ContainerStatus::Creating,
            None,
            &self.bundle,
            container_dir,
        )?;
        container.save()?;
        Ok(container)
    }
}
