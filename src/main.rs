use std::env;
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
            let dt = tzmath::DateTime::parse(ts).map_err(|e| e.to_string())?;
            let offset = tzmath::Offset::parse(offset).map_err(|e| e.to_string())?;
            Ok(dt.with_offset(offset).to_string())
        }
        [cmd, ts, duration] if cmd == "add" => {
            let dt = tzmath::DateTime::parse(ts).map_err(|e| e.to_string())?;
            let delta = tzmath::parse_duration(duration).map_err(|e| e.to_string())?;
            Ok(dt.add_seconds(delta).to_string())
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

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  tzmath convert <timestamp> <offset>");
    eprintln!("      tzmath convert 2024-03-10T14:30:00-05:00 +09:00");
    eprintln!("  tzmath add <timestamp> <duration>");
    eprintln!("      tzmath add 2024-03-10T14:30:00-05:00 3h30m");
    eprintln!("      tzmath add 2024-03-10T14:30:00-05:00 -90m");
    eprintln!("  tzmath diff <from> <to>");
    eprintln!("      tzmath diff 2024-03-10T14:30:00-05:00 2024-03-10T18:00:00-05:00");
}
