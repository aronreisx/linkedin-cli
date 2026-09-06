use std::io::Write;
use clap::ValueEnum;
use serde::Serialize;
use serde_json::Value;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    Ndjson,
    Tsv,
}

#[derive(Debug, Clone, Serialize)]
pub struct DryRunAction {
    pub resource: String,
    pub action: String,
    pub target: Option<String>,
    pub parameters: Value,
    pub dry_run: bool,
}

/// Safely writes a line to stdout with robust SIGPIPE handling (exits 0 if downstream pipe closes)
pub fn print_stdout_line(line: &str) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    if let Err(e) = writeln!(stdout, "{line}") {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            // Downstream reader (e.g. head, fzf, jq) terminated. Exit cleanly.
            std::process::exit(0);
        }
        return Err(e);
    }
    let _ = stdout.flush();
    Ok(())
}

/// Streams serializable items line-by-line in real-time as NDJSON
pub fn stream_ndjson_items<T: Serialize, I: IntoIterator<Item = T>>(items: I) -> Result<(), serde_json::Error> {
    for item in items {
        let json_line = serde_json::to_string(&item)?;
        let _ = print_stdout_line(&json_line);
    }
    Ok(())
}

/// Helper to render data in the requested OutputFormat (Table, Json, Ndjson, Tsv)
pub fn render_data<T: Serialize>(
    data: &T,
    format: OutputFormat,
    human_summary: &str,
) -> Result<(), serde_json::Error> {
    match format {
        OutputFormat::Json => {
            let json_str = serde_json::to_string_pretty(data)?;
            let _ = print_stdout_line(&json_str);
        }
        OutputFormat::Ndjson => {
            let val = serde_json::to_value(data)?;
            match val {
                Value::Array(items) => {
                    for item in items {
                        let line = serde_json::to_string(&item)?;
                        let _ = print_stdout_line(&line);
                    }
                }
                other => {
                    let line = serde_json::to_string(&other)?;
                    let _ = print_stdout_line(&line);
                }
            }
        }
        OutputFormat::Tsv => {
            let val = serde_json::to_value(data)?;
            print_tsv(&val);
        }
        OutputFormat::Table => {
            let _ = print_stdout_line(human_summary);
        }
    }
    Ok(())
}

/// Helper to render dry-run simulation payload with zero network side-effects
pub fn render_dry_run(action: DryRunAction, format: OutputFormat) -> Result<(), serde_json::Error> {
    let target_display = action.target.as_deref().unwrap_or("none");
    let params_summary = serde_json::to_string_pretty(&action.parameters).unwrap_or_default();
    let summary = format!(
        "[DRY-RUN] Simulating {} {} on target: {}\nParameters:\n{}\nZero network requests or CDP events dispatched.",
        action.resource, action.action, target_display, params_summary
    );
    render_data(&action, format, &summary)
}

/// Legacy compatibility helper
pub fn render_output<T: Serialize>(data: &T, as_json: bool, human_summary: &str) -> Result<(), serde_json::Error> {
    let format = if as_json { OutputFormat::Json } else { OutputFormat::Table };
    render_data(data, format, human_summary)
}

/// Legacy compatibility helper
pub fn render_value(val: &Value, as_json: bool, default_summary: &str) -> Result<(), serde_json::Error> {
    let format = if as_json { OutputFormat::Json } else { OutputFormat::Table };
    render_data(val, format, default_summary)
}

fn print_tsv(val: &Value) {
    match val {
        Value::Array(items) => {
            if items.is_empty() {
                return;
            }
            if let Some(first_obj) = items[0].as_object() {
                let keys: Vec<&String> = first_obj.keys().collect();
                let header = keys.iter().map(|k| k.as_str()).collect::<Vec<_>>().join("\t");
                let _ = print_stdout_line(&header);
                for item in items {
                    if let Some(obj) = item.as_object() {
                        let row: Vec<String> = keys
                            .iter()
                            .map(|k| format_tsv_field(obj.get(*k)))
                            .collect();
                        let _ = print_stdout_line(&row.join("\t"));
                    }
                }
            } else {
                for item in items {
                    let _ = print_stdout_line(&format_tsv_field(Some(item)));
                }
            }
        }
        Value::Object(map) => {
            let _ = print_stdout_line("key\tvalue");
            for (k, v) in map {
                let line = format!("{k}\t{}", format_tsv_field(Some(v)));
                let _ = print_stdout_line(&line);
            }
        }
        primitive => {
            let line = format_tsv_field(Some(primitive));
            let _ = print_stdout_line(&line);
        }
    }
}

fn format_tsv_field(val: Option<&Value>) -> String {
    match val {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.replace(['\t', '\n', '\r'], " "),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Array(arr)) => {
            serde_json::to_string(arr).unwrap_or_default().replace('\t', " ")
        }
        Some(Value::Object(obj)) => {
            serde_json::to_string(obj).unwrap_or_default().replace('\t', " ")
        }
    }
}
