use std::io;
use std::process::ExitCode;

use rs_for_reals2ipc::BasicConfig;

fn io_envkey2str(key: &'static str) -> impl Fn() -> String {
    move || std::env::var(key).unwrap_or_default()
}

fn io_foref_filename() -> impl Fn() -> String {
    io_envkey2str("ENV_FOR_FILENAME")
}

fn io_column_name() -> impl Fn() -> String {
    io_envkey2str("ENV_COLUMN_NAME")
}

fn io_basic_config() -> impl Fn() -> BasicConfig {
    move || {
        let foref_filename: String = io_foref_filename()();
        let column_name: String = io_column_name()();
        BasicConfig {
            foref_filename,
            column_name,
        }
    }
}

fn io_main() -> impl Fn() -> Result<(), io::Error> {
    || {
        let cfg: BasicConfig = io_basic_config()();
        cfg.stdin2packed2fs2ref2iter2stdout()
    }
}

fn sub() -> Result<(), io::Error> {
    io_main()()
}

fn main() -> ExitCode {
    sub().map(|_| ExitCode::SUCCESS).unwrap_or_else(|e| {
        eprintln!("{e}");

        let cfg: BasicConfig = io_basic_config()();
        eprintln!("cfg: {cfg:#?}");

        ExitCode::FAILURE
    })
}
