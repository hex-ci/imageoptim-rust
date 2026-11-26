//! SVGO compiled into the app: the upstream v1 plugin configuration bundled
//! with webpack runs on an embedded QuickJS runtime, so no Node.js install is
//! required (upstream shells out to `node svgo.js`).

use anyhow::{Context as _, Result, bail};
use rquickjs::{Context as JsContext, Runtime};

const BUNDLE: &str = include_str!("../../../third_party/src/svgo/svgo.js");

/// Runs SVGO with the upstream `defaults` (lossless) or `defaults + lossy`
/// plugin lists and `full: true`.
pub fn optimize(data: &[u8], lossy: bool) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(data).context("SVG is not UTF-8")?;
    let rt = Runtime::new().context("quickjs runtime")?;
    let ctx = JsContext::full(&rt).context("quickjs context")?;
    ctx.with(|ctx| {
        ctx.eval::<(), _>(BUNDLE)
            .context("evaluating svgo bundle")?;
        let input = serde_json::to_string(text).context("encoding svg input")?;
        ctx.eval::<(), _>(format!("io_optimize({input}, {})", u8::from(lossy)))
            .context("calling io_optimize")?;
        Ok::<(), anyhow::Error>(())
    })?;
    // Promise microtasks run as QuickJS jobs; drive them outside the context
    // borrow (execute_pending_job needs the runtime unlocked).
    let mut jobs = 0usize;
    loop {
        let done = ctx
            .with(|ctx| ctx.eval::<bool, _>("io_done"))
            .unwrap_or(false);
        if done {
            break;
        }
        if !rt
            .execute_pending_job()
            .map_err(|e| anyhow::anyhow!("quickjs job: {e:?}"))?
        {
            break;
        }
        jobs += 1;
        if jobs > 1_000_000 {
            bail!("svgo promise did not settle");
        }
    }
    ctx.with(|ctx| {
        if let Some(err) = ctx.eval::<Option<String>, _>("io_error").unwrap_or(None) {
            bail!("svgo: {err}");
        }
        let out = ctx
            .eval::<Option<String>, _>("io_result")
            .context("reading svgo result")?
            .context("svgo produced no output")?;
        Ok(out.into_bytes())
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn svgo_optimizes_svg() {
        let input = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <!-- comment -->
  <rect x="10" y="10" width="80" height="80" fill="#ff0000" stroke="black"/>
</svg>
"##;
        let out = super::optimize(input, false).unwrap();
        assert!(out.len() < input.len());
        assert!(!String::from_utf8_lossy(&out).contains("comment"));
        let lossy = super::optimize(input, true).unwrap();
        assert!(lossy.len() <= out.len());
    }
}
