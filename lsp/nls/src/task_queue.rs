use std::{
    collections::{HashSet, VecDeque},
    str::FromStr,
};

use anyhow::Result;
use log::debug;
use lsp_server::{Message, Notification, RequestId};
use lsp_types::{
    CancelParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, NumberOrString, Url,
    notification::{
        Cancel, DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
        Notification as NotificationTrait,
    },
    request::{ExecuteCommand, Request as RequestTrait},
};

#[derive(Debug, Clone)]
pub enum Task {
    /// Handle an LSP request
    HandleRequest(lsp_server::Request),
    /// Handle a document synchronization notification.
    HandleDocumentSync(DocumentSync),
    /// Publish diagnostics for a file
    Diagnostics(DiagnosticsRequest),
    /// Cancel a request.
    CancelRequest(RequestId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Normal,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticsRequest {
    pub uri: Url,
    pub priority: Priority,
}

#[derive(Debug, Clone)]
pub enum DocumentSync {
    Open(DidOpenTextDocumentParams),
    Close(DidCloseTextDocumentParams),
    Change(DidChangeTextDocumentParams),
}

/// Something that is either an LSP request or a document synchronization notification.
enum ReqOrSync {
    Request(lsp_server::Request),
    DocumentSync(DocumentSync),
}

pub struct TaskQueue {
    /// LSP requests and document synchronization tasks that need to be handled. They are processed
    /// in the order that they are received.
    ///
    /// The ordering is important because many requests reference a position in a document. If a
    /// request is sent and a document change notification follows it, then applying the document
    /// change could cause the request's position to point somewhere the user did not expect. It
    /// may often be the case that if the document changes after a request is made then the
    /// response to the request may no longer be useful, but the LSP spec recommends that the
    /// server should not make the decision to cancel a request in most cases, instead the client
    /// should send a cancellation notification if the response is no longer needed.
    request_or_sync: VecDeque<ReqOrSync>,
    /// The set of files that are currently open in memory. This is used to prioritize tasks
    /// for open files.
    open_files: HashSet<Url>,
    /// The set of files that need updated diagnostics published for them.
    stale_diagnostics: HashSet<Url>,
    cancelled_requests: HashSet<RequestId>,
}

impl TaskQueue {
    pub fn new() -> Self {
        TaskQueue {
            request_or_sync: VecDeque::new(),
            open_files: HashSet::new(),
            stale_diagnostics: HashSet::new(),
            cancelled_requests: HashSet::new(),
        }
    }

    /// Takes an LSP request or notification and handles queueing an appropriate task.
    pub fn queue_message(&mut self, msg: Message) -> Result<()> {
        match msg {
            Message::Request(req) => {
                self.request_or_sync.push_back(ReqOrSync::Request(req));
                Ok(())
            }
            Message::Notification(notification) => self.queue_notification(notification),
            Message::Response(_) => Ok(()),
        }
    }

    /// Add a task to apply a document synchronization notification
    fn add_sync_task(&mut self, task: DocumentSync) {
        self.request_or_sync
            .push_back(ReqOrSync::DocumentSync(task));
    }

    /// Add a task to publish updated diagnostics on a file.
    pub fn add_diagnostics_task(&mut self, uri: Url) {
        self.stale_diagnostics.insert(uri);
    }

    /// Takes an LSP notification and adds a task if needed.
    fn queue_notification(&mut self, notification: Notification) -> Result<()> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params =
                    serde_json::from_value::<DidOpenTextDocumentParams>(notification.params)?;
                self.open_files.insert(params.text_document.uri.clone());
                self.add_sync_task(DocumentSync::Open(params));
            }
            DidCloseTextDocument::METHOD => {
                let params =
                    serde_json::from_value::<DidCloseTextDocumentParams>(notification.params)?;
                self.open_files.remove(&params.text_document.uri);
                self.add_sync_task(DocumentSync::Close(params));
            }
            DidChangeTextDocument::METHOD => {
                let params =
                    serde_json::from_value::<DidChangeTextDocumentParams>(notification.params)?;
                self.add_sync_task(DocumentSync::Change(params));
            }
            Cancel::METHOD => {
                let params = serde_json::from_value::<CancelParams>(notification.params)?;
                let id = match params.id {
                    NumberOrString::Number(id) => id.into(),
                    NumberOrString::String(id) => id.into(),
                };
                self.cancelled_requests.insert(id);
            }
            method => debug!("No handler for notification type {}", method),
        };
        Ok(())
    }

    /// Get the next file that needs its diagnostics refreshed. This has a preference for files
    /// that are open in memory since those files are currently being edited and are of greater
    /// relevance. If no open files are out of date it will pick another out of date file in an
    /// arbitrary order.
    fn next_diagnostic(&mut self) -> Option<Task> {
        if let Some(uri) = self
            .open_files
            .iter()
            .find(|file| self.stale_diagnostics.contains(file))
            .cloned()
        {
            self.stale_diagnostics.remove(&uri);
            Some(Task::Diagnostics(DiagnosticsRequest {
                uri,
                priority: Priority::High,
            }))
        } else if let Some(uri) = self.stale_diagnostics.iter().next().cloned() {
            self.stale_diagnostics.remove(&uri);
            Some(Task::Diagnostics(DiagnosticsRequest {
                uri,
                priority: Priority::Normal,
            }))
        } else {
            None
        }
    }

    /// Get the next request to handle if there's one pending.
    fn next_request_or_sync(&mut self) -> Option<Task> {
        // There's a few types of requests that the queue will handle differently. These categories
        // will determine that handling.
        enum RequestCategory {
            /// ExecuteCommand request for the "eval" command
            EvalCommand(Url),
            /// Any request extending TextDocumentPositionParams in the LSP spec.
            /// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocumentPositionParams
            TextDocumentUri(Url),
            Other,
        }

        fn categorize(req: &lsp_server::Request) -> RequestCategory {
            let is_eval_command = req.method == ExecuteCommand::METHOD
                && req.params["command"].as_str() == Some("eval");
            if is_eval_command {
                let uri = req.params["arguments"]
                    .as_array()
                    .and_then(|args| args.first())
                    .and_then(|arg| arg["uri"].as_str())
                    .and_then(|url| Url::from_str(url).ok());
                return match uri {
                    Some(uri) => RequestCategory::EvalCommand(uri),
                    None => RequestCategory::Other,
                };
            };

            // This should get the document URI for any request type extending TextDocumentPositionParams
            // in the LSP spec. https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocumentPositionParams
            let uri = req.params["textDocument"]["uri"]
                .as_str()
                .and_then(|uri| Url::from_str(uri).ok());
            match uri {
                Some(uri) => RequestCategory::TextDocumentUri(uri),
                None => RequestCategory::Other,
            }
        }
        match self.request_or_sync.pop_front() {
            Some(ReqOrSync::DocumentSync(task)) => Some(Task::HandleDocumentSync(task)),
            Some(ReqOrSync::Request(req)) => {
                if self.cancelled_requests.contains(&req.id) {
                    self.cancelled_requests.remove(&req.id);
                    return Some(Task::CancelRequest(req.id));
                }
                let task = match categorize(&req) {
                    RequestCategory::EvalCommand(uri) => {
                        // The eval command publishes the new diagnostics for a file
                        // synchronously, so if there were a pending update to the diagnostics for a
                        // file, we can remove it.
                        self.stale_diagnostics.remove(&uri);
                        Task::HandleRequest(req)
                    }
                    RequestCategory::TextDocumentUri(uri) => {
                        // Most of the request handlers expect the file to be parsed and typechecked
                        // already before the handler is run. If the request specifies a file URI, and
                        // the diagnostics on that URI are out of date, then the diagnostics task will
                        // need to be run first before the request can be handled. That will ensure
                        // that parsing and typechecking are current before the request handler is run.
                        if self.stale_diagnostics.contains(&uri) {
                            self.stale_diagnostics.remove(&uri);

                            // Put the request back into its same place in the queue so it gets
                            // handled after parsing and typechecking are finished.
                            self.request_or_sync.push_front(ReqOrSync::Request(req));
                            Task::Diagnostics(DiagnosticsRequest {
                                uri,
                                priority: Priority::High,
                            })
                        } else {
                            Task::HandleRequest(req)
                        }
                    }
                    RequestCategory::Other => Task::HandleRequest(req),
                };
                Some(task)
            }
            None => None,
        }
    }

    pub fn next_task(&mut self) -> Option<Task> {
        let task = self
            .next_request_or_sync()
            .or_else(|| self.next_diagnostic());
        if task.is_none() {
            // If there's nothing in the queue, then any requests in the cancelled requests must
            // have already been processed. We can clean up whatever is remaining.
            self.cancelled_requests.clear();
        }
        task
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use assert_matches::assert_matches;
    use lsp_server::Request;
    use lsp_types::request::{ExecuteCommand, GotoDefinition, Request as RequestTrait};
    use serde_json::json;

    #[test]
    fn queue_request() {
        let mut queue = TaskQueue::new();
        let req = Request::new(
            123.into(),
            ExecuteCommand::METHOD.into(),
            json!({
                "command":"eval",
                "arguments":[{"uri": "file:///test.ncl"}]
            }),
        );

        queue.queue_message(Message::Request(req)).unwrap();
        let task = queue.next_task().unwrap();
        assert_matches!(task, Task::HandleRequest(_));
        // Check if the task has been removed.
        assert!(queue.next_task().is_none());
    }

    #[test]
    fn queue_doc_sync() {
        let mut queue = TaskQueue::new();
        let notification = Notification::new(
            DidChangeTextDocument::METHOD.into(),
            json!({
                "textDocument":{
                    "uri":"file:///test.ncl",
                    "version":8
                },
                "contentChanges":[{"text":"1 + 1"}]
            }),
        );
        queue
            .queue_message(Message::Notification(notification))
            .unwrap();
        let task = queue.next_task().unwrap();
        assert_matches!(task, Task::HandleDocumentSync(DocumentSync::Change(_)));
        // Check if the task has been removed.
        assert!(queue.next_task().is_none());
    }

    #[test]
    fn queue_diagnostics() {
        let mut queue = TaskQueue::new();
        let uri: Url = "file:///test.ncl".parse().unwrap();
        queue.add_diagnostics_task(uri.clone());
        let task = queue.next_task().unwrap();
        match task {
            Task::Diagnostics(req) => assert_eq!(
                req,
                DiagnosticsRequest {
                    uri,
                    priority: Priority::Normal
                }
            ),
            _ => panic!("Got wrong task type"),
        }
        // Check if the task has been removed.
        assert!(queue.next_task().is_none());
    }

    #[test]
    fn check_task_priority() {
        let mut queue = TaskQueue::new();

        queue.add_diagnostics_task("file:///test3.ncl".parse().unwrap());

        let req = Request::new(
            123.into(),
            ExecuteCommand::METHOD.into(),
            json!({
                "command":"eval",
                "arguments":[{"uri": "file:///test2.ncl"}]
            }),
        );
        queue.queue_message(Message::Request(req)).unwrap();

        let notification = Notification::new(
            DidCloseTextDocument::METHOD.into(),
            json!({
                "textDocument":{
                    "uri":"file:///test1.ncl"
                },
            }),
        );
        queue
            .queue_message(Message::Notification(notification))
            .unwrap();

        // Diagnostics should be handled after other task types even if the task was pushed first.
        // Other task types should be processed in the order they're received.
        assert_matches!(queue.next_task().unwrap(), Task::HandleRequest(_));
        assert_matches!(queue.next_task().unwrap(), Task::HandleDocumentSync(_));
        assert_matches!(queue.next_task().unwrap(), Task::Diagnostics(_));
        assert!(queue.next_task().is_none());
    }

    #[test]
    fn didopen_notification_opens_file() {
        let mut queue = TaskQueue::new();
        let notification = Notification::new(
            DidOpenTextDocument::METHOD.into(),
            json!({
                "textDocument":{
                    "uri":"file:///test.ncl",
                    "version":1,
                    "languageId":"nickel",
                    "text":"1 + 1"
                },
            }),
        );
        queue
            .queue_message(Message::Notification(notification))
            .unwrap();
        assert!(
            queue
                .open_files
                .contains(&"file:///test.ncl".parse().unwrap())
        )
    }

    #[test]
    fn didclose_notification_closes_file() {
        let mut queue = TaskQueue::new();
        queue.open_files.insert("file:///test.ncl".parse().unwrap());
        let notification = Notification::new(
            DidCloseTextDocument::METHOD.into(),
            json!({
                "textDocument":{
                    "uri":"file:///test.ncl"
                },
            }),
        );
        queue
            .queue_message(Message::Notification(notification))
            .unwrap();
        assert!(queue.open_files.is_empty());
    }

    #[test]
    fn open_file_diagnostics_are_high_priority() {
        let mut queue = TaskQueue::new();
        let uri: Url = "file:///test.ncl".parse().unwrap();
        queue.open_files.insert(uri.clone());
        queue.add_diagnostics_task(uri.clone());

        // If open files weren't prioritized it's likely we'd see one of these other files rather
        // than /test.ncl
        for x in 1..500 {
            queue.add_diagnostics_task(format!("file:///{}.ncl", x).parse().unwrap());
        }

        let task = queue.next_task().unwrap();
        match task {
            Task::Diagnostics(req) => assert_eq!(
                req,
                DiagnosticsRequest {
                    uri,
                    priority: Priority::High
                }
            ),
            _ => panic!("Wrong task type"),
        }
    }

    #[test]
    fn diagnostics_requests_for_the_same_document_are_evaluated_once() {
        let mut queue = TaskQueue::new();
        let uri: Url = "file:///test.ncl".parse().unwrap();
        queue.add_diagnostics_task(uri.clone());
        queue.add_diagnostics_task(uri.clone());
        queue.add_diagnostics_task(uri.clone());

        queue.next_task().unwrap();
        assert!(queue.next_task().is_none());
    }

    #[test]
    fn eval_command_clears_diagnostic_request() {
        let mut queue = TaskQueue::new();
        queue.add_diagnostics_task("file:///test.ncl".parse().unwrap());
        let req = Request::new(
            123.into(),
            ExecuteCommand::METHOD.into(),
            json!({
                "command":"eval",
                "arguments":[{"uri": "file:///test.ncl"}]
            }),
        );

        queue.queue_message(Message::Request(req)).unwrap();
        let task = queue.next_task().unwrap();
        assert_matches!(task, Task::HandleRequest(_));
        // There should no longer be a diagnostic task to return here.
        assert!(queue.next_task().is_none());
    }

    #[test]
    fn sync_tasks_for_the_same_document_are_processed_in_order() {
        // With the exception of obsolete document changes being skipped,
        // synchronization tasks for a single document should be processed in the order that
        // they're received. If notifications are for different documents, there's no guarantee
        // made about the order in which they're processed.
        let mut queue = TaskQueue::new();
        let open = Notification::new(
            DidOpenTextDocument::METHOD.into(),
            json!({
                "textDocument":{
                    "uri":"file:///test.ncl",
                    "version":1,
                    "languageId":"nickel",
                    "text":"1 + 1"
                },
            }),
        );

        let close = Notification::new(
            DidCloseTextDocument::METHOD.into(),
            json!({
                "textDocument":{
                    "uri":"file:///test.ncl"
                },
            }),
        );
        queue.queue_message(Message::Notification(open)).unwrap();
        queue.queue_message(Message::Notification(close)).unwrap();

        assert_matches!(
            queue.next_task().unwrap(),
            Task::HandleDocumentSync(DocumentSync::Open(_))
        );
        assert_matches!(
            queue.next_task().unwrap(),
            Task::HandleDocumentSync(DocumentSync::Close(_))
        );
    }

    #[test]
    fn parse_and_typecheck_run_before_request() {
        let mut queue = TaskQueue::new();
        let req = Request::new(
            123.into(),
            GotoDefinition::METHOD.into(),
            json!({
                "textDocument":{
                    "uri":"file:///test.ncl"
                },
                "position":{"line":11,"character":20}
            }),
        );
        queue.queue_message(Message::Request(req)).unwrap();
        queue.add_diagnostics_task("file:///test.ncl".parse().unwrap());
        assert_matches!(queue.next_task().unwrap(), Task::Diagnostics(_));
        assert_matches!(queue.next_task().unwrap(), Task::HandleRequest(_));
    }

    #[test]
    fn cancel_request_int_id() {
        let mut queue = TaskQueue::new();
        let req = Request::new(
            123.into(),
            ExecuteCommand::METHOD.into(),
            json!({
                "command":"eval",
                "arguments":[{"uri": "file:///test.ncl"}]
            }),
        );

        let cancel = Notification::new(
            Cancel::METHOD.into(),
            json!({
                "id": 123
            }),
        );

        queue.queue_message(Message::Request(req)).unwrap();
        queue.queue_message(Message::Notification(cancel)).unwrap();
        let task = queue.next_task().unwrap();
        match task {
            Task::CancelRequest(id) => assert_eq!(id, 123.into()),
            _ => panic!("Wrong task type"),
        }
        // The request should actually be cancelled now.
        assert!(queue.next_task().is_none());
    }

    #[test]
    fn cancel_request_str_id() {
        let mut queue = TaskQueue::new();
        let req = Request::new(
            "123".to_string().into(),
            ExecuteCommand::METHOD.into(),
            json!({
                "command":"eval",
                "arguments":[{"uri": "file:///test.ncl"}]
            }),
        );

        let cancel = Notification::new(
            Cancel::METHOD.into(),
            json!({
                "id": "123"
            }),
        );

        queue.queue_message(Message::Request(req)).unwrap();
        queue.queue_message(Message::Notification(cancel)).unwrap();
        let task = queue.next_task().unwrap();
        match task {
            Task::CancelRequest(id) => assert_eq!(id, "123".to_string().into()),
            _ => panic!("Wrong task type"),
        }
        // The request should actually be cancelled now.
        assert!(queue.next_task().is_none());
    }

    #[test]
    fn clear_cancelled_requests() {
        let mut queue = TaskQueue::new();
        let id = 123;
        let req = Request::new(
            id.into(),
            ExecuteCommand::METHOD.into(),
            json!({
                "command":"eval",
                "arguments":[{"uri": "file:///test.ncl"}]
            }),
        );

        let cancel = Notification::new(
            Cancel::METHOD.into(),
            json!({
                "id": id
            }),
        );

        queue.queue_message(Message::Request(req)).unwrap();
        // The request has already been taken out of the queue before the cancel notification
        // arrives.
        let task = queue.next_task().unwrap();
        assert_matches!(task, Task::HandleRequest(_));

        queue.queue_message(Message::Notification(cancel)).unwrap();
        assert!(queue.next_task().is_none());

        assert_eq!(0, queue.cancelled_requests.len());
    }
}
