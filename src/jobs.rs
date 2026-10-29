use anyhow::Result;
use std::sync::mpsc::{Receiver, Sender};

use crate::model::{RenderedPage, Tab};

pub enum JobMessage {
    Progress {
        tab: Tab,
        value: f32,
        text: String,
    },
    Row {
        tab: Tab,
        index: usize,
        status: String,
    },
    Done {
        tab: Tab,
        text: String,
        message: String,
    },
    Failed {
        tab: Tab,
        error: String,
    },
    Preview {
        generation: u64,
        page: u32,
        image: RenderedPage,
    },
}

#[derive(Clone)]
pub struct JobCtx {
    pub tab: Tab,
    pub tx: Sender<JobMessage>,
    pub ctx: eframe::egui::Context,
}

impl JobCtx {
    pub fn progress(&self, value: f32, text: impl Into<String>) {
        let _ = self.tx.send(JobMessage::Progress {
            tab: self.tab,
            value: value.clamp(0.0, 1.0),
            text: text.into(),
        });
        self.ctx.request_repaint();
    }

    pub fn row(&self, index: usize, status: impl Into<String>) {
        let _ = self.tx.send(JobMessage::Row {
            tab: self.tab,
            index,
            status: status.into(),
        });
        self.ctx.request_repaint();
    }

    pub fn done(&self, text: impl Into<String>, message: impl Into<String>) {
        let _ = self.tx.send(JobMessage::Done {
            tab: self.tab,
            text: text.into(),
            message: message.into(),
        });
        self.ctx.request_repaint();
    }
}

pub struct JobBus {
    pub tx: Sender<JobMessage>,
    pub rx: Receiver<JobMessage>,
}

impl JobBus {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self { tx, rx }
    }

    pub fn spawn(
        &self,
        ctx: eframe::egui::Context,
        tab: Tab,
        f: impl FnOnce(JobCtx) -> Result<()> + Send + 'static,
    ) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let jc = JobCtx {
                tab,
                tx: tx.clone(),
                ctx: ctx.clone(),
            };
            if let Err(err) = f(jc.clone()) {
                let _ = tx.send(JobMessage::Failed {
                    tab,
                    error: format!("{err:#}"),
                });
                ctx.request_repaint();
            }
        });
    }

    pub fn spawn_preview(
        &self,
        ctx: eframe::egui::Context,
        bytes: Vec<u8>,
        page_index: usize,
        page: u32,
        generation: u64,
        scale: f32,
    ) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            match crate::pdfcore::render_page(&bytes, page_index, scale) {
                Ok(image) => {
                    let _ = tx.send(JobMessage::Preview {
                        generation,
                        page,
                        image,
                    });
                }
                Err(err) => {
                    let _ = tx.send(JobMessage::Failed {
                        tab: Tab::Edit,
                        error: format!("Önizleme oluşturulamadı: {err:#}"),
                    });
                }
            }
            ctx.request_repaint();
        });
    }
}
