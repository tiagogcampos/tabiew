use clap::Parser;
use polars::frame::DataFrame;
use polars::io::csv::read::{CsvParseOptions, CsvReadOptions};
use polars::lazy::frame::IntoLazy;
use polars::prelude::ParquetReader;
use polars::prelude::*;
use polars_sql::SQLContext;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stderr};
use tabiew::app::{AppResult, StatusBar, Tabular};
use tabiew::args::{Args, InferSchema};
use tabiew::command::{CommandList, ExecutionTable};
use tabiew::event::{Event, EventHandler};
use tabiew::handler::handle_key_events;
use tabiew::theme::Styler;
use tabiew::tui::Tui;
use tabiew::utils::infer_schema_safe;

fn load_parquet_file(args: &Args) -> DataFrame {
    let file = std::fs::File::open(&args.file_name).unwrap();

    ParquetReader::new(file).finish().unwrap()
}

fn load_json_file(args: &Args) -> DataFrame {
    let file = std::fs::File::open(&args.file_name).unwrap();

    JsonReader::new(file).finish().unwrap()
}

fn load_csv_file(args: &Args) -> DataFrame {
    let data_frame = {
        let mut df = CsvReadOptions::default()
            .with_ignore_errors(args.ignore_errors)
            .with_infer_schema_length((&args.infer_schema).into())
            .with_has_header(!args.no_header)
            .with_parse_options(
                CsvParseOptions::default()
                    .with_quote_char((args.quote_char as u8).into())
                    .with_separator(args.separator as u8),
            )
            .try_into_reader_with_file_path(Some(args.file_name.clone()))
            .unwrap()
            .finish()
            .unwrap();
        if matches!(args.infer_schema, InferSchema::Safe) {
            infer_schema_safe(&mut df);
        }
        df
    };

    data_frame
}

fn main() -> AppResult<()> {
    // Parse CLI
    let args = Args::parse();

    let data_frame = match args.file_name.extension().unwrap().to_str() {
        Some("parquet") => load_parquet_file(&args),
        Some("csv") => load_csv_file(&args),
        Some("json") => load_json_file(&args),
        _ => unimplemented!(),
    };

    // Setup the SQLContext
    let mut sql_context = SQLContext::new();
    sql_context.register("df", data_frame.clone().lazy());

    // Instantiate app
    let tabular = Tabular::new(data_frame);
    let status_bar = StatusBar::default();

    // Command handling
    let exec_tbl = CommandList::default().into_exec();

    // Initialize the terminal user interface.
    let backend = CrosstermBackend::new(io::stderr());
    let terminal = Terminal::new(backend)?;
    let events = EventHandler::new(250);
    let mut tui = Tui::new(terminal, events);
    tui.init()?;

    // Run the main loop
    match args.theme {
        tabiew::args::AppTheme::Monokai => main_loop::<tabiew::theme::Monokai>(
            &mut tui,
            tabular,
            status_bar,
            sql_context,
            exec_tbl,
        )?,
        tabiew::args::AppTheme::Terminal => main_loop::<tabiew::theme::Terminal>(
            &mut tui,
            tabular,
            status_bar,
            sql_context,
            exec_tbl,
        )?,
    }

    // Exit the user interface.
    tui.exit()?;
    Ok(())
}

fn main_loop<Theme: Styler>(
    tui: &mut Tui<CrosstermBackend<Stderr>>,
    mut tabular: Tabular,
    mut status_bar: StatusBar,
    mut sql_context: SQLContext,
    exec_tbl: ExecutionTable,
) -> AppResult<()> {
    let mut running = true;

    // Start the main loop.
    while running {
        // Render the user interface.
        tui.draw::<Theme>(&mut tabular, &mut status_bar)?;
        // Handle events.
        match tui.events.next()? {
            Event::Tick => {
                tabular.tick();
                status_bar.tick();
            }
            Event::Key(key_event) => {
                #[cfg(target_os = "windows")]
                {
                    use crossterm::event::KeyEventKind;
                    if matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                        handle_key_events(
                            key_event,
                            &mut tabular,
                            &mut status_bar,
                            &mut sql_context,
                            &mut running,
                            &exec_tbl,
                        )?
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    handle_key_events(
                        key_event,
                        &mut tabular,
                        &mut status_bar,
                        &mut sql_context,
                        &mut running,
                        &exec_tbl,
                    )?
                }
            }
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
        }
    }
    Ok(())
}
