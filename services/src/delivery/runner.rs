//! DeliveryRunner - polls the queue and dispatches deliveries
use std::sync::Arc;
use std::time::Duration;

use tracing::{error, info};

use super::queue::{DeliveryQueue, QueuedDelivery};

pub struct DeliveryRunner {
    queue: Arc<DeliveryQueue>,
    poll_interval: Duration,
}

impl DeliveryRunner {
    pub fn new(queue: Arc<DeliveryQueue>) -> Self {
        Self {
            queue,
            poll_interval: Duration::from_secs(2),
        }
    }

    // start the runner as a background tokio task
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            info!("DeliveryRunner started");
            loop {
                self.run_once().await;
                tokio::time::sleep(self.poll_interval).await;
            }
        })
    }

    // process all pending deliveries once
    pub async fn run_once(&self) {
        let items = self.queue.dequeue();
        for item in items {
            let id = item.id.clone();
            match self.dispatch(&item).await {
                Ok(()) => {
                    if let Err(e) = self.queue.ack(&id) {
                        error!("Failed to ack delivery {}: {}", &id[..8], e);
                    }
                }
                Err(e) => {
                    if let Err(fe) = self.queue.fail(&id, &e.to_string()) {
                        error!("Failed to record failure for {}: {}", &id[..8], fe);
                    }
                }
            }
        }
    }

    // dispatch a single delivery to its channel
    async fn dispatch(&self, item: &QueuedDelivery) -> anyhow::Result<()> {
        match item.channel.as_str() {
            "terminal" => self.dispatch_terminal(item),
            "file" => self.dispatch_file(item),
            other => Err(anyhow::anyhow!("Unknown channel: {}", other)),
        }
    }

    fn dispatch_terminal(&self, item: &QueuedDelivery) -> anyhow::Result<()> {
        // bell character + message for terminal notification
        print!("\x07");
        println!("\n[notify] {}\n", item.text);
        Ok(())
    }

    fn dispatch_file(&self, item: &QueuedDelivery) -> anyhow::Result<()> {
        // `to` field is the file path to write
        let path = &item.to;
        std::fs::write(path, &item.text)?;
        info!("Delivery written to file: {}", path);
        Ok(())
    }
}
