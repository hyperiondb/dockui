use std::sync::RwLock;

use bollard::Docker;

use crate::types::{ContainerInfo, ContainerRef};

pub struct Registry {
    containers: RwLock<Vec<ContainerInfo>>,
}

impl Registry {
    pub fn new() -> Self {
        Registry {
            containers: RwLock::new(Vec::new()),
        }
    }

    pub async fn refresh(&self, docker: &Docker) {
        match crate::docker::list_containers(docker, true).await {
            Ok(list) => {
                if let Ok(mut guard) = self.containers.write() {
                    *guard = list;
                }
            }
            Err(e) => tracing::warn!("container list refresh failed: {e}"),
        }
    }

    pub fn list(&self) -> Vec<ContainerInfo> {
        self.containers
            .read()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    pub fn running(&self) -> Vec<ContainerRef> {
        self.containers
            .read()
            .map(|g| {
                g.iter()
                    .filter(|c| c.state == "running")
                    .map(|c| ContainerRef { id: c.id.clone() })
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}
