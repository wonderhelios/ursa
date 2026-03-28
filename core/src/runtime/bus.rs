//! EventBus — typed pub/sub for inter-module communication.

use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::warn;

pub use crate::pipeline::gvrc::WorkflowEvent;

// ===== Event types =====
#[derive(Debug, Clone)]
pub enum Event {
    /// User submitted input to the pipeline
    UserInput { content: String },
    /// Pipeline started processing
    PipelineStarted { iteration: usize },
    /// A tool was called
    ToolCalled { name: String, args: String },
    /// A tool returned a result
    ToolCompleted { name: String, result_len: usize },
    /// Pipeline finished
    PipelineCompleted {
        iterations: usize,
        response_len: usize,
    },
    /// A notification was enqueued
    NotificationQueued { message: String },
    /// Workflow-specific events (GVRC architecture)
    Workflow(WorkflowEvent),
}

// ===== EventBus =====

const BUS_CAPACITY: usize = 512;

/// Thread-safe event bus. Clone to share across components.
#[derive(Clone)]
pub struct EventBus {
    sender: Arc<broadcast::Sender<Event>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(BUS_CAPACITY);
        Self {
            sender: Arc::new(sender),
        }
    }

    // publish an event to all current subscribers
    pub fn publish(&self, event: Event) -> usize {
        self.sender.send(event).unwrap_or_default()
    }

    // publish a workflow event (convenience method)
    pub fn publish_workflow(&self, event: WorkflowEvent) -> usize {
        self.publish(Event::Workflow(event))
    }

    // subscribe to the event stream
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    // listen to workflow events only
    pub fn listen_workflow<F>(&self, mut handler: F) -> tokio::task::JoinHandle<()>
    where
        F: FnMut(WorkflowEvent) + Send + 'static,
    {
        let mut rx = self.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(Event::Workflow(wf)) => handler(wf),
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("EventBus receiver lagged, skipped {} events", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    }

    // spawn a background task that processes events from this bus
    pub fn listen<F>(&self, mut handler: F) -> tokio::task::JoinHandle<()>
    where
        F: FnMut(Event) + Send + 'static,
    {
        let mut rx = self.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => handler(event),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("EventBus receiver lagged, skipped {} events", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
