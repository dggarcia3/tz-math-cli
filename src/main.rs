use std::env;
use std::io::{self, BufRead};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match run(&args[1..]) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("error: {msg}");
            eprintln!();
            print_usage();
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<String, String> {
    match args {
        [cmd, ts, offset] if cmd == "convert" => {
            if ts == "-" {
                run_stdin(offset, convert_one)
            } else {
                convert_one(ts, offset)
            }
        }
        [cmd, ts, duration] if cmd == "add" => {
            if ts == "-" {
                run_stdin(duration, add_one)
            } else {
                add_one(ts, duration)
            }
        }
        [cmd, from, to] if cmd == "diff" => {
            let from = tzmath::DateTime::parse(from).map_err(|e| e.to_string())?;
            let to = tzmath::DateTime::parse(to).map_err(|e| e.to_string())?;
            let elapsed = to.to_epoch_seconds() - from.to_epoch_seconds();
            Ok(tzmath::format_duration(elapsed))
        }
        [cmd, ..] => Err(format!("unknown command '{cmd}'")),
        [] => Err("no command given".to_string()),
    }
}

fn convert_one(ts: &str, offset: &str) -> Result<String, String> {
    let dt = tzmath::DateTime::parse(ts).map_err(|e| e.to_string())?;
    let offset = tzmath::Offset::parse(offset).map_err(|e| e.to_string())?;
    Ok(dt.with_offset(offset).to_string())
}

fn add_one(ts: &str, duration: &str) -> Result<String, String> {
    let dt = tzmath::DateTime::parse(ts).map_err(|e| e.to_string())?;
    let delta = tzmath::parse_duration(duration).map_err(|e| e.to_string())?;
    Ok(dt.add_seconds(delta).to_string())
}

/// Reads timestamps from stdin, one per line, applies `op` to each along
/// with the fixed second argument, and joins the results with newlines.
/// Blank lines are skipped. A bad line aborts the whole run rather than
/// emitting partial output, matching the all-or-nothing behavior of the
/// single-timestamp commands.
fn run_stdin<F>(arg: &str, op: F) -> Result<String, String>
where
    F: Fn(&str, &str) -> Result<String, String>,
{
    let stdin = io::stdin();
    let mut out_lines = Vec::new();
    for (i, line) in stdin.lock().lines().enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let result = op(line, arg).map_err(|msg| format!("line {}: {msg}", i + 1))?;
        out_lines.push(result);
    }
    if out_lines.is_empty() {
        return Err("no timestamps read from stdin".to_string());
    }
    Ok(out_lines.join("\n"))
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  tzmath convert <timestamp> <offset>");
    eprintln!("      tzmath convert 2024-03-10T14:30:00-05:00 +09:00");
    eprintln!("  tzmath add <timestamp> <duration>");
    eprintln!("      tzmath add 2024-03-10T14:30:00-05:00 3h30m");
    eprintln!("      tzmath add 2024-03-10T14:30:00-05:00 -90m");
    eprintln!("  tzmath diff <from> <to>");
    eprintln!("      tzmath diff 2024-03-10T14:30:00-05:00 2024-03-10T18:00:00-05:00");
    eprintln!("  use '-' as <timestamp> to read one timestamp per line from stdin");
    eprintln!("  with convert or add; results are printed one per line");
    eprintln!("      cat timestamps.txt | tzmath convert - +09:00");
}
