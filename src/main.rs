use std::collections::HashSet;
use std::env::Args;
use std::os::unix::prelude::CommandExt;
use std::process::exit;
use std::{env, io};

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use naive_opt::Search;
use tui::style::{Color, Modifier, Style};
use tui::widgets::{BorderType, Wrap};
use tui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Span, Spans},
    widgets::{Block, Borders, Clear, Paragraph},
    Terminal,
};

enum Mode {
    Insert,
    Select,
}

struct Config {
    query: Option<String>,
    command: String,
}

fn main() -> Result<(), io::Error> {
    let args = Config::new(env::args());
    let command = args.command;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut query = String::new();
    let mut results: Vec<String> = Vec::new();
    let mut mode = Mode::Insert;
    let mut selected = 0;
    let mut info_scroll = 0;
    let mut info: Vec<Spans> = Vec::new();
    let mut redraw = true;

    let mut installed_cache: HashSet<usize> = HashSet::new();
    let mut cached_pages: HashSet<usize> = HashSet::new();

    if args.query.is_some() {
        terminal.set_cursor(0, 0)?;
        query = args.query.unwrap();
        let packages = search(&query, &command);

        for line in packages.lines() {
            results.push(line.to_owned());
        }

        mode = Mode::Select;
        terminal.set_cursor(1, 4)?;
    }

    terminal.clear()?;

    loop {
        let mut line = selected;
        let mut should_skip = false;

        let size = terminal.size();
        if size.is_err() {
            continue;
        }
        let size = size.unwrap();
        if size.height <= 5 {
            should_skip = true;
        }

        let page = selected / (size.height - 5);
        let skipped = page * (size.height - 5);
        line -= skipped;

        if redraw {
            redraw = false;
            terminal.draw(|s| {
                let chunks = Layout::default()
                    .constraints([Constraint::Min(3), Constraint::Percentage(100)].as_ref())
                    .split(size);

                let para = Paragraph::new(Spans::from(vec![
                    Span::styled("Search: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(&*query),
                ]))
                .block(
                    Block::default()
                        .title(Span::styled(
                            "parui",
                            Style::default().add_modifier(Modifier::BOLD),
                        ))
                        .title_alignment(Alignment::Center)
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded),
                )
                .alignment(Alignment::Left);
                s.render_widget(para, chunks[0]);

                let para = Paragraph::new(format_results(
                    &results,
                    selected,
                    size.height as usize,
                    (results.len() as f32 + 1f32).log10().ceil() as usize,
                    skipped as usize,
                    &mut installed_cache,
                    &mut cached_pages,
                    &command,
                ))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded),
                )
                .alignment(Alignment::Left);
                s.render_widget(para, chunks[1]);

                if results.is_empty() {
                    let area = Rect {
                        x: size.width / 4 + 1,
                        y: size.height / 2 - 2,
                        width: size.width / 2,
                        height: 4,
                    };
                    let no_results = Paragraph::new("No results, try searching for something else")
                        .block(
                            Block::default()
                                .title(Span::styled(
                                    "No Results",
                                    Style::default().add_modifier(Modifier::BOLD),
                                ))
                                .title_alignment(Alignment::Center)
                                .borders(Borders::ALL)
                                .border_type(BorderType::Rounded),
                        )
                        .wrap(Wrap { trim: true })
                        .alignment(Alignment::Center);
                    s.render_widget(Clear, area);
                    s.render_widget(no_results, area);
                } else {
                    let area = Rect {
                        x: size.width / 2,
                        y: 4,
                        width: size.width / 2,
                        height: size.height - 5,
                    };
                    let info = Paragraph::new(if info.is_empty() {
                        {
                            let mut actions = vec![Spans::from(Span::styled(
                                "Press ENTER to show package information",
                                Style::default()
                                    .fg(Color::Green)
                                    .add_modifier(Modifier::BOLD),
                            ))];
                            if installed_cache.contains(&(selected as usize)) {
                                actions.push(Spans::from(Span::styled(
                                    "Press Shift-R to uninstall this package".to_owned(),
                                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                                )));
                            }
                            actions
                        }
                    } else {
                        info.iter().skip(info_scroll).cloned().collect()
                    })
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded),
                    )
                    .wrap(Wrap { trim: true })
                    .alignment(Alignment::Left);
                    s.render_widget(Clear, area);
                    s.render_widget(info, area);
                }
            })?;
        }

        if should_skip {
            continue;
        }

        match mode {
            Mode::Insert => {
                terminal.set_cursor(query.len() as u16 + 9, 1)?;
            }
            Mode::Select => {
                terminal.set_cursor(1, line + 4)?;
            }
        }

        terminal.show_cursor()?;

        match event::read()? {
            Event::Key(k) => match mode {
                Mode::Insert => match k.code {
                    KeyCode::Esc => {
                        selected = 0;
                        redraw = true;
                        mode = Mode::Select;
                    }
                    KeyCode::Backspace => {
                        if !query.is_empty() {
                            query.pop();
                        }
                        redraw = true;
                    }
                    KeyCode::Char(c) => match c {
                        'c' => {
                            if k.modifiers == KeyModifiers::CONTROL {
                                disable_raw_mode()?;
                                execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;
                                terminal.clear()?;
                                terminal.set_cursor(0, 0)?;

                                return Ok(());
                            }
                            query.push(c);
                            redraw = true;
                        }
                        'w' => {
                            if k.modifiers == KeyModifiers::CONTROL {
                                let mut chars = query.chars().rev();
                                for c in chars.by_ref() {
                                    match c {
                                        ' ' | '-' | '_' => break,
                                        _ => (),
                                    }
                                }
                                let chars = chars.rev().collect::<String>();
                                query = chars;
                                redraw = true;
                            } else {
                                query.push(c);
                                redraw = true;
                            }
                        }
                        _ => {
                            query.push(c);
                            redraw = true;
                        }
                    },
                    KeyCode::Enter => {
                        results.clear();
                        installed_cache.clear();
                        cached_pages.clear();
                        info.clear();
                        selected = 0;
                        terminal.set_cursor(1, 4)?;
                        let packages = search(&query, &command);

                        for line in packages.lines() {
                            results.push(line.to_owned());
                        }

                        redraw = true;
                        mode = Mode::Select;
                    }
                    _ => redraw = true,
                },
                Mode::Select => match k.code {
                    KeyCode::Up => {
                        if k.modifiers == KeyModifiers::CONTROL {
                            if info_scroll > 0 {
                                info_scroll -= 1;
                                redraw = true;
                            }
                        } else {
                            if selected > 0 {
                                selected -= 1;
                                info.clear();
                            } else {
                                selected = results.len() as u16 - 1;
                                info.clear();
                            }
                            redraw = true;
                        }
                    }
                    KeyCode::Down => {
                        if k.modifiers == KeyModifiers::CONTROL {
                            if !info.is_empty() {
                                info_scroll += 1;
                                redraw = true;
                            }
                        } else {
                            let result_count = results.len();
                            if result_count > 1 && selected < result_count as u16 - 1 {
                                selected += 1;
                                info.clear();
                            } else {
                                selected = 0;
                                info.clear();
                            }
                            redraw = true;
                        }
                    }
                    KeyCode::Left => {
                        let per_page = size.height - 5;

                        if selected >= per_page && results.len() > per_page as usize {
                            selected -= per_page;
                            info.clear();
                            redraw = true;
                        }
                    }
                    KeyCode::Right => {
                        let size = terminal.size().unwrap();
                        let per_page = size.height - 5;

                        if selected < results.len() as u16 - per_page
                            && results.len() > per_page as usize
                        {
                            selected += per_page;
                            info.clear();
                            redraw = true;
                        }
                    }
                    KeyCode::Char(c) => match c {
                        'j' => {
                            if k.modifiers == KeyModifiers::CONTROL {
                                if !info.is_empty() {
                                    info_scroll += 1;
                                    redraw = true;
                                }
                            } else {
                                let result_count = results.len();
                                if result_count > 1 && selected < result_count as u16 - 1 {
                                    selected += 1;
                                    info.clear();
                                } else {
                                    selected = 0;
                                    info.clear();
                                }
                                redraw = true;
                            }
                        }
                        'k' => {
                            if k.modifiers == KeyModifiers::CONTROL {
                                if info_scroll > 0 {
                                    info_scroll -= 1;
                                    redraw = true;
                                }
                            } else {
                                if selected > 0 {
                                    selected -= 1;
                                    info.clear();
                                } else {
                                    selected = results.len() as u16 - 1;
                                    info.clear();
                                }
                                redraw = true;
                            }
                        }
                        'h' => {
                            let size = terminal.size().unwrap();
                            let per_page = size.height - 5;

                            if selected >= per_page && results.len() > per_page as usize {
                                selected -= per_page;
                                info.clear();
                                redraw = true;
                            }
                        }
                        'l' => {
                            let size = terminal.size().unwrap();
                            let per_page = size.height - 5;

                            if selected < results.len() as u16 - per_page
                                && results.len() > per_page as usize
                            {
                                selected += per_page;
                                info.clear();
                                redraw = true;
                            }
                        }
                        'q' => {
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;
                            terminal.clear()?;
                            terminal.set_cursor(0, 0)?;

                            return Ok(());
                        }
                        'i' => {
                            redraw = true;
                            mode = Mode::Insert;
                        }
                        'c' => {
                            if k.modifiers == KeyModifiers::CONTROL {
                                disable_raw_mode()?;
                                execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;
                                terminal.clear()?;
                                terminal.set_cursor(0, 0)?;

                                return Ok(());
                            }
                            query.push(c);
                            redraw = true;
                        }
                        'g' => {
                            selected = 0;
                            redraw = true;
                        }
                        'G' => {
                            selected = results.len() as u16 - 1;
                            redraw = true;
                        }
                        'R' => {
                            if installed_cache.contains(&(selected as usize)) {
                                disable_raw_mode()?;
                                execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;

                                terminal.clear()?;
                                terminal.set_cursor(0, 0)?;
                                terminal.show_cursor()?;
                                let mut cmd = std::process::Command::new(command);
                                cmd.arg("-R").arg(&(results[selected as usize]));
                                cmd.exec();

                                return Ok(());
                            }
                        }

                        _ => redraw = true,
                    },
                    KeyCode::Enter => {
                        if info.is_empty() {
                            info = get_info(
                                &(results[selected as usize]),
                                selected as usize,
                                &installed_cache,
                                &command,
                            );
                            redraw = true;
                        } else {
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;

                            terminal.clear()?;
                            terminal.set_cursor(0, 0)?;
                            terminal.show_cursor()?;
                            let mut cmd = std::process::Command::new(command);
                            cmd.arg("-S").arg(&(results[selected as usize]));
                            cmd.exec();

                            return Ok(());
                        }
                    }
                    _ => redraw = true,
                },
            },
            _ => redraw = true,
        }
    }
}

fn search(query: &str, command: &str) -> String {
    let mut cmd = std::process::Command::new(command);
    cmd.args(["--topdown", "-Ssq", query]);
    let output = cmd.output().unwrap();
    String::from_utf8(output.stdout).unwrap()
}

fn format_results(
    lines: &[String],
    selected: u16,
    height: usize,
    pad_to: usize,
    skip: usize,
    installed_cache: &mut HashSet<usize>,
    cached_pages: &mut HashSet<usize>,
    command: &str,
) -> Vec<Spans<'static>> {
    let index_style = Style::default().fg(Color::Gray);
    let installed_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    // let installed_selected_style = installed_style.add_modifier(Modifier::UNDERLINED);
    let installed_selected_style = Style::default()
        .bg(Color::Red)
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let uninstalled_style = Style::default()
        .fg(Color::LightBlue)
        .add_modifier(Modifier::BOLD);
    // let uninstalled_selected_style = uninstalled_style.add_modifier(Modifier::UNDERLINED);
    let uninstalled_selected_style = Style::default()
        .bg(Color::Red)
        .fg(Color::Blue)
        .add_modifier(Modifier::BOLD);

    let lines: Vec<String> = lines.iter().skip(skip).take(height - 5).cloned().collect();
    is_installed(&lines, skip, installed_cache, cached_pages, command);

    let skip = skip + 1;
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let index = i + skip;
            let index_string = index.to_string();
            Spans::from(vec![
                Span::styled(index_string, index_style),
                Span::raw(" ".repeat(pad_to - (index as f32 + 1f32).log10().ceil() as usize + 1)),
                Span::styled(
                    line.clone(),
                    if installed_cache.contains(&(i + skip - 1)) {
                        if selected == (index - 1) as u16 {
                            installed_selected_style
                        } else {
                            installed_style
                        }
                    } else if selected == (index - 1) as u16 {
                        uninstalled_selected_style
                    } else {
                        uninstalled_style
                    },
                ),
            ])
        })
        .collect()
}

fn get_info(
    query: &String,
    index: usize,
    installed_cache: &HashSet<usize>,
    command: &str,
) -> Vec<Spans<'static>> {
    let mut cmd = std::process::Command::new(command);

    let mut info = vec![Spans::from(Span::styled(
        "Press ENTER again to install this package",
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    ))];

    if installed_cache.contains(&index) {
        info.push(Spans::from(Span::styled(
            "Press Shift-R to uninstall this package".to_owned(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        cmd.arg("-Qi").arg(query);
    } else {
        cmd.arg("-Si").arg(query);
    };
    info.push(Spans::default());

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    for line in stdout.lines().map(|c| c.to_owned()) {
        if line.contains(':') {
            let (key, value) = line.split_once(':').unwrap();
            info.push(Spans::from(vec![
                Span::styled(
                    key.to_owned() + ":",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(value.to_owned()),
            ]));
        } else {
            info.push(Spans::from(Span::raw(line.to_owned())));
        }
    }

    info
}

fn is_installed(
    queries: &[String],
    skip: usize,
    installed_cache: &mut HashSet<usize>,
    cached_pages: &mut HashSet<usize>,
    command: &str,
) {
    if cached_pages.contains(&skip) {
        return;
    }

    let mut cmd = std::process::Command::new(command);
    cmd.arg("-Qq");
    cmd.args(queries);

    let output = cmd.output().unwrap().stdout;
    let output = String::from_utf8(output).unwrap();
    let mut index;
    for (i, query) in queries.iter().enumerate() {
        index = i + skip;
        let is_installed = output.includes(&(query.to_owned() + "\n"));
        if is_installed {
            installed_cache.insert(index);
        }
    }
    cached_pages.insert(skip);
}

fn print_help() {
    println!(concat!(
        "Usage: parui [OPTION]... QUERY\n",
        "Search for QUERY in the Arch User Repository.\n",
        "Example:\n",
        "    parui -p=yay rustup\n\n",
        "Options:\n",
        "    -p=<PROGRAM>\n",
        "        Selects program used to search AUR\n",
        "        Not guaranteed to work well\n",
        "        Default: paru\n",
        "    -h\n",
        "        Print this help and exit"
    ));
    exit(0);
}

impl Config {
    pub fn new(args: Args) -> Self {
        let mut query: Option<String> = None;
        let mut command = String::from("paru");

        for arg in args.skip(1) {
            match arg.as_str() {
                "-h" | "--help" => print_help(),
                _ => {
                    let arg = arg.clone();
                    if arg.starts_with("-p=") {
                        let stripped = arg.clone().chars().skip(3).collect::<String>();
                        command = stripped;
                        continue;
                    }
                    if query.is_some() {
                        query = Some(query.unwrap() + " " + &arg);
                    } else {
                        query = Some(arg.to_owned());
                    }
                }
            }
        }

        Self { query, command }
    }
}
