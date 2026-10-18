//! HTTP seam + the real reqwest transport.
//!
//! The seam keeps the whole state machine unit-testable with a
//! scripted fake (the DiskGenie pattern — the live-E2E workflow then
//! exercises the real transport against the deployed Worker). The
//! trait is always compiled; the reqwest implementation is gated on
//! the `client` feature (see Cargo.toml for the cross-check rationale).

/// The transport seam: POST JSON with headers → (status, body).
pub trait LicenseHttp: Send + Sync {
    /// # Errors
    /// String transport error (network/TLS) — callers map to the
    /// soft `Network` failure class.
    fn post_json(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<(u16, String), String>;
}

/// The real transport (reqwest blocking, 30 s timeout; rustls — no
/// native TLS dependency in the WebView process).
#[cfg(feature = "client")]
pub struct ReqwestLicense {
    client: reqwest::blocking::Client,
}

#[cfg(feature = "client")]
impl ReqwestLicense {
    /// # Errors
    /// String error when the TLS client cannot be constructed.
    pub fn new() -> Result<Self, String> {
        use std::time::Duration;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| format!("license http client: {e}"))?;
        Ok(Self { client })
    }
}

#[cfg(feature = "client")]
impl Default for ReqwestLicense {
    fn default() -> Self {
        Self::new().expect("reqwest blocking client constructs")
    }
}

#[cfg(feature = "client")]
impl LicenseHttp for ReqwestLicense {
    fn post_json(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<(u16, String), String> {
        let mut req = self
            .client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            // WIRE CONTRACT: the Worker's request guard pins this UA
            // exactly — BAD_UA otherwise.
            .header(reqwest::header::USER_AGENT, "DiskWizard-License-Client/1");
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = req
            .body(body.to_string())
            .send()
            .map_err(|e| format!("network: {e}"))?;
        let status = resp.status().as_u16();
        let text = resp.text().map_err(|e| format!("network: {e}"))?;
        Ok((status, text))
    }
}
