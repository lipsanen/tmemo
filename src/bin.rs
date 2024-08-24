use std::{
    backtrace::Backtrace,
    io::{self, Stdout},
    panic,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cmd::Cli;
use crossterm::{
    event, execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use tmemo::{cmd, render, state};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cmd::Cli::parse(std::env::args());

    if args.command.is_some() {
        args.run();
        return Ok(());
    }

    run(args)?;
    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>, Box<dyn std::error::Error>> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    Ok(terminal)
}

fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn set_panic_hook() {
    panic::set_hook(Box::new(|info| {
        let mut terminal = setup_terminal().unwrap();
        restore_terminal(&mut terminal).unwrap();
        let bt = Backtrace::capture();
        eprintln!("{}\nBacktrace: {}", info, bt);
    }));
}

fn run(cmd: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let mut state = state::ApplicationState::new();
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    state.process(state::TmemoStateAction::Seed(seed));

    if cmd.from_stdin {
        state.load_from_stdin();
    } else if cmd.state_from_file.is_some() {
        state.load_from_statefile(cmd.state_from_file.unwrap());
    } else {
        state.load_from_file();
    }

    let mut terminal = setup_terminal()?;
    set_panic_hook();
    loop {
        terminal.draw(|frame| {
            render::render_app(frame, &state.current_state);
        })?;
        let res = handle_events(&mut state);

        if res.is_err() || state.current_state.wants_to_quit {
            break;
        }
    }
    if !cmd.from_stdin {
        state.process(state::TmemoStateAction::SaveToJson);
    }
    restore_terminal(&mut terminal)?;
    Ok(())
}

fn handle_events(state: &mut state::ApplicationState) -> Result<(), Box<dyn std::error::Error>> {
    if event::poll(Duration::from_millis(250))? {
        let event = event::read()?;
        let action = state::to_action(event, state);

        if let Some(action) = action {
            state.process(action);
        }
    }

    Ok(())
}
