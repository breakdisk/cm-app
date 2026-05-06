use std::sync::Arc;
use anyhow::Context;
use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use tera::Tera;
use tokio::sync::Mutex;

pub struct PdfRenderer {
    browser: Arc<Mutex<Browser>>,
    tera:    Tera,
}

impl PdfRenderer {
    pub async fn new(template_dir: &str) -> anyhow::Result<Self> {
        // Initialise templates first — if the directory is missing, fail fast before launching Chrome.
        let glob = format!("{template_dir}/**/*.html");
        let tera = Tera::new(&glob).context("Failed to load Tera templates")?;

        let config = BrowserConfig::builder()
            .no_sandbox()
            .build()
            .map_err(|e| anyhow::anyhow!("BrowserConfig error: {e}"))?;
        let (browser, mut handler) = Browser::launch(config).await
            .context("Failed to launch Chromium")?;

        tokio::spawn(async move {
            loop {
                if handler.next().await.is_none() { break; }
            }
        });

        Ok(Self {
            browser: Arc::new(Mutex::new(browser)),
            tera,
        })
    }

    pub async fn render_invoice(
        &self,
        context: &tera::Context,
    ) -> anyhow::Result<Vec<u8>> {
        let html = self.tera.render("invoice.html", context)
            .context("Tera render failed")?;

        let page = {
            let browser = self.browser.lock().await;
            browser.new_page("about:blank").await
                .context("Failed to open Chrome tab")?
        };

        let pdf_bytes = async {
            page.set_content(html).await.context("Failed to set page content")?;
            let pdf_opts = chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams::default();
            page.pdf(pdf_opts).await.context("Failed to print PDF")
        }.await;

        page.close().await.ok();

        pdf_bytes
    }
}
