use std::{future::Future, sync::Arc};

use libs::{anyhow::Error, once_cell};

use crate::cache::BerlinDirProvider;

struct Deferred<T>(once_cell::unsync::OnceCell<T>);

impl<T> Default for Deferred<T> {
    fn default() -> Self {
        Self(once_cell::unsync::OnceCell::default())
    }
}

impl<T> Deferred<T> {
    pub fn get_or_try_init(&self, create: impl FnOnce() -> Result<T, Error>) -> Result<&T, Error> {
        self.0.get_or_try_init(create)
    }

    pub fn get_or_init(&self, create: impl FnOnce() -> T) -> &T {
        self.0.get_or_init(create)
    }

    pub async fn get_or_try_init_async(
        &self,
        create: impl Future<Output = Result<T, Error>>,
    ) -> Result<&T, Error> {
        if self.0.get().is_none() {
            // todo(dsherret): it would be more ideal if this enforced a
            // single executor and then we could make some initialization
            // concurrent
            let val = create.await?;
            _ = self.0.set(val);
        }
        Ok(self.0.get().unwrap())
    }
}

#[derive(Default)]
struct CliFactoryServices {
    deno_dir_provider: Deferred<Arc<BerlinDirProvider>>,
}

pub struct CliFactory {
    //watcher_communicator: Option<Arc<WatcherCommunicator>>,
    options: Arc<CliOptions>,
    services: CliFactoryServices,
}

impl CliFactory {}
