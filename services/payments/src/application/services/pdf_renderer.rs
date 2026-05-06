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

        let glob = format!("{template_dir}/**/*.html");
        let tera = Tera::new(&glob).context("Failed to load Tera templates")?;

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

        let browser = self.browser.lock().await;
        let page = browser.new_page("about:blank").await
            .context("Failed to open Chrome tab")?;

        page.set_content(html).await.context("Failed to set page content")?;

        let pdf_opts = chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams::default();
        let pdf_bytes = page.pdf(pdf_opts).await.context("Failed to print PDF")?;
        page.close().await.ok();

        Ok(pdf_bytes)
    }
}
