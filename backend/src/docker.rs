use std::collections::HashMap;

use anyhow::Result;
use bollard::container::ListContainersOptions;
use bollard::system::EventsOptions;
use bollard::Docker;
use futures_util::Stream;

use crate::types::ContainerInfo;

pub fn connect() -> Result<Docker> {
    let docker = Docker::connect_with_local_defaults()?;
    Ok(docker)
}

pub async fn list_containers(docker: &Docker, all: bool) -> Result<Vec<ContainerInfo>> {
    let opts = ListContainersOptions::<String> {
        all,
        ..Default::default()
    };
    let summaries = docker.list_containers(Some(opts)).await?;
    let mut out = Vec::with_capacity(summaries.len());
    for s in summaries {
        let id = match s.id {
            Some(id) => id,
            None => continue,
        };
        let name = s
            .names
            .and_then(|n| n.into_iter().next())
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_else(|| crate::util::short_id(&id).to_string());
        out.push(ContainerInfo {
            id,
            name,
            image: s.image.unwrap_or_default(),
            state: s.state.unwrap_or_default(),
            status: s.status.unwrap_or_default(),
            created: s.created.unwrap_or(0),
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

#[derive(Clone, Debug)]
pub struct ContainerEvent {
    pub action: String,
    pub id: String,
}

pub fn container_events(
    docker: &Docker,
) -> impl Stream<Item = Result<ContainerEvent, bollard::errors::Error>> {
    let mut filters = HashMap::new();
    filters.insert("type".to_string(), vec!["container".to_string()]);
    let opts = EventsOptions::<String> {
        since: None,
        until: None,
        filters,
    };
    let stream = docker.events(Some(opts));
    futures_util::StreamExt::filter_map(stream, |res| async move {
        match res {
            Ok(ev) => {
                let action = ev.action?;
                let id = ev.actor.and_then(|a| a.id)?;
                Some(Ok(ContainerEvent { action, id }))
            }
            Err(e) => Some(Err(e)),
        }
    })
}
