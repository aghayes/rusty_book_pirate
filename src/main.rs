use irc::client::prelude::*;
use futures::prelude::*;
use std::{io, io::prelude::*, fs::File, time::Duration, sync::Arc, collections::HashMap};
use tui::Terminal;
use tui::backend::CrosstermBackend;
use tui::layout::{Layout, Alignment, Direction, Constraint};
use tui::style::{Style, Color, Modifier};
use tui::widgets::{List, ListItem, Block, Borders, Paragraph};
use crossterm::event::{Event, poll, read, KeyCode, KeyModifiers};
use regex::Regex;
use tokio::sync::{mpsc, Mutex};
mod defs;
use crate::defs::{States, Item, StateList, Args};
mod dcc;
use crate::dcc::{Dcc};

async fn connect(server_pair: &str, name: String) -> Result<(irc::client::Client, irc::client::ClientStream, String, String), irc::error::Error>{
    let server_pair: Vec<&str> = server_pair.split(", ").collect();
    let server = Some(server_pair[0].to_string());
    let channel = server_pair[1].to_string();
    let channels = vec![channel.clone()];
    let config = Config {
        nickname: Some(name.to_string()),
        server,
        port: Some(6667),
        use_tls: Some(false),
        channels,
        ..Config::default()
    };
    let mut client = Client::from_config(config).await?;
    client.identify().unwrap();

    
    let mut stream = client.stream().unwrap();

    //waits for server to log us in
    loop{ 
        let m = match stream.next().await{
            Some(m) => m,
            _ => Err(irc::error::Error::Io{0: io::Error::new(io::ErrorKind::Other, "irc error")})
        };
        let message = m?;
        if let Command::NOTICE(_s, msg) = &message.command{
             if msg.contains("Welcome to #ebooks"){break}   
        }
    }
    return Ok((client, stream, channel, server_pair[2].to_string()))
}
const CON_OPTIONS: [&str; 2] = [
    "irc.irchighway.net, #ebooks, @Search",
    "irc.irchighway.net, #ebooks, @Searchook",
];


async fn irc_get(chan: Arc<String> ,cmd: &str, client: Arc<irc::client::Client>, stream: Arc<Mutex<mpsc::UnboundedReceiver<Result<irc::proto::Message, irc::error::Error>>>>, se: &Regex, timeout: u64, name: &str) -> Result<String, irc::error::Error>{
    let mut stream = stream.lock().await;
    client.send_privmsg(chan, cmd)?;
    let start = std::time::Instant::now();
    loop{
        if timeout != 0 && start.elapsed().as_secs() == timeout{
            return Err(irc::error::Error::Io{0: io::Error::new(io::ErrorKind::TimedOut, "message timed out")})
        }
        let m = match stream.recv().await.transpose()?{
            Some(m) => m,
            None => continue,
        };
        if let Command::PRIVMSG(nm, msg) = &m.command {
            if nm == name && se.is_match(msg) {
                return Ok(String::from(msg))
            }
        }
    }
}

fn parse_search(zip_file: std::io::Cursor<Vec<u8>>) -> Result<Vec<String>, io::Error>{
    //let zip_file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(zip_file)?;
    let file = archive.by_index(0)?;
    let search_results = io::BufReader::new(file).lines();
    let results: Vec<String> = search_results.filter_map(|line| {
        match line{
            Ok(l) if !l.is_empty() && &l[..1] == "!" => Some(l),
            _ => None,
        }
    }).collect();
    Ok(results)
}

async fn get_book(args: Args<'_>){
    std::fs::create_dir_all(&args.path).unwrap();
    let dcc = match Dcc::from_msg(match &irc_get(args.chan, &args.cmd, args.client, args.stream, &args.se, 60, &args.name).await{
        Ok(m) => m, 
        Err(_) => {let mut state = args.state.lock().await; *state = States::Failed; return},
        }){
        Ok(v) => v,
        Err(_) => {let mut state = args.state.lock().await; *state = States::Failed; return},
    };
    let file_name = if let Some(s) = args.se.captures(&dcc.msg){
        s[1].to_string()
    }else {
        let mut state = args.state.lock().await; *state = States::Failed; return
    };
    let f = dcc.get_file().unwrap();
    let mut file = File::create(format!["{}/{}", args.path, file_name]).unwrap();
    file.write_all(&f).unwrap();
    let mut state = args.state.lock().await;
    *state = States::Got;
}
async fn get_search(args: Args<'_>){
    let dcc = Dcc::from_msg(match &irc_get(args.chan.clone() ,&args.cmd, args.client.clone(), args.stream.clone(), &args.se, 60, &args.name).await{
        Ok(m) => m,
        Err(_) => {let mut state = args.state.lock().await; *state = States::SearchFailed; return}
    }).unwrap();
    let f = dcc.get_file().unwrap();
    let file = std::io::Cursor::new(f);
    let mut search_results = parse_search(file).unwrap();

    let mut sources: HashMap<String, bool> = HashMap::new();

    if let Some(users) = args.client.list_users(&args.chan){
        for user in &users{
            sources.insert(user.get_nickname().to_string(), true);
        }
        search_results = search_results.iter().filter_map(|result| { let result = result.to_string();
            let name = result.split_whitespace().collect::<Vec<&str>>()[0].chars().collect::<Vec<char>>()[1..].iter().collect::<String>();
            if sources.contains_key(&name){
                Some(result)
            }else{
                None
            }
        }).collect::<Vec<String>>();
    }

    let raw_items = search_results.clone();
    let mut items = args.items.lock().await;
    *items = StateList::from(search_results.iter().map(|i| Item{item: ListItem::new(i.clone()), cmd: i.clone()}).collect());
    let mut file_names = args.file_names.lock().await;
    *file_names = raw_items;
    let mut state = args.state.lock().await;
    *state = States::Results;
}
async fn pingpong(mut stream: irc::client::ClientStream,  new_stream: mpsc::UnboundedSender<Result<irc::proto::Message, irc::error::Error>>, client: Arc<irc::client::Client>){
    while let Some(m) = stream.next().await{
        let m = m.unwrap();
        if let irc::proto::Command::PING(_,_) = m.command{
            client.send_pong("pong").unwrap();
        }else{
            new_stream.send(Ok(m)).unwrap();
        }
    }
}
#[tokio::main]
async fn main () {
    let NAME = format!["RBP{}", uuid::Uuid::new_v4().as_fields().1];
    let re = Regex::new(r#"(?:(?i)SEND\s"*)((?i).*[a-z])(?:(:?(?i)"*)\s[0-9])"#).unwrap();

    let cons = CON_OPTIONS.iter().map(|c|{Item{item: ListItem::new((&c).to_string()), cmd: c.to_string()}}).collect();
    let connections: Arc<Mutex<StateList<Item>>> = Arc::new(Mutex::new(StateList::from(cons)));

    let items: Arc<Mutex<StateList<Item>>> = Arc::new(Mutex::new(StateList::new()));
    let file_names: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));


    let state: Arc<Mutex<States>> = Arc::new(Mutex::from(States::Connect));

    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();
    let client: Option<irc::client::Client>;
    let stream: Option<irc::client::ClientStream>;
    let channel: Option<String>;
    let search_command: Option<String>;
    loop{
        let mut loop_state = state.lock().await;
        let mut loop_items = connections.lock().await;
        match &*loop_state{
            States::Connect =>{
                let selected = if let Some(i) = loop_items.state.selected(){Some(loop_items.items[i].cmd.clone())}else{None};
                terminal.draw(move |f| {
                    let chunks = Layout::default()
                                                .direction(Direction::Vertical)
                                                .constraints(
                                                    [
                                                        Constraint::Percentage(90),
                                                        Constraint::Percentage(10),
                                                    ].as_ref()
                                                )
                                                .split(f.size());
                    let con_list = List::new(loop_items.items
                            .clone()
                            .iter()
                            .map(|i: &Item| i.item.clone()).collect::<Vec<ListItem>>())
                        .block(Block::default().title("Servers:").borders(Borders::ALL))
                        .style(Style::default().fg(Color::Gray))
                        .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Green))
                        .highlight_symbol(">>");
                    f.render_stateful_widget(con_list, chunks[0], &mut loop_items.state);
                    let commands = Paragraph::new(" Select: arrows | Connect: enter | Quit: q")
                                                        .style(Style::default().fg(Color::Gray))
                                                        .alignment(Alignment::Left);
                                                    f.render_widget(commands, chunks[1]);

                }).unwrap();
                if poll(Duration::from_millis(100)).unwrap(){
                    match read().unwrap(){
                        Event::Key(k) => {
                            match k.code{
                                KeyCode::Char(c) if c == 'q' || c == 'Q' =>{return},
                                KeyCode::Down =>{connections.lock().await.next();},
                                KeyCode::Up =>{connections.lock().await.previous();},
                                KeyCode::Enter => {if let Some(s) = selected{*loop_state = States::Connecting(s);}},
                                _ =>(),
                            }
                        },
                        Event::Mouse(_)=> continue,
                        Event::Resize(_, _) => continue,
                    }
                }
            },
            States::Connecting(server) =>{
                terminal.draw(|f|{
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(
                            [
                                Constraint::Percentage(90),
                                Constraint::Percentage(10),
                            ].as_ref()
                        )
                        .split(f.size());
                    let prompt = Paragraph::new(format!["\n\nConnecting to: {}\nThis might take a while...", &server])
                                            .block(Block::default().title("Connecting:").borders(Borders::ALL))
                                            .style(Style::default().fg(Color::Gray))
                                            .alignment(Alignment::Center);
                                        f.render_widget(prompt, chunks[0]);
                }).unwrap();
                let (c, s, chan, cmd) = connect(server, NAME.clone()).await.unwrap(); client = Some(c); stream = Some(s); channel = Some(chan); search_command = Some(cmd); *loop_state = States::Connected; break
            },
            _=>(),
        }

    }
    let client = Arc::from(client.unwrap());
    let (tx,rx) = mpsc::unbounded_channel();
    let channel: Arc<String> = Arc::from(channel.unwrap());
    let search_command: Arc<String> = Arc::from(search_command.unwrap());
    tokio::task::spawn(pingpong(stream.unwrap(), tx, client.clone()));
    let stream = Arc::from(Mutex::from(rx));
    let mut search_term = String::new();
    let mut dir_path = String::from("./");
    loop{
        let mut loop_state = state.lock().await;
        match &*loop_state {
            States::Failed =>  {
                                terminal.draw(
                                    move |f|{
                                        let chunks = Layout::default()
                                        .direction(Direction::Vertical)
                                        .constraints(
                                            [
                                                Constraint::Percentage(90),
                                                Constraint::Percentage(10),
                                            ].as_ref()
                                        )
                                        .split(f.size());
                                        let prompt = Paragraph::new("\n\nFailed to dowload book.\nEnter to select a different copy or s to enter a new search.")
                                            .block(Block::default().title("Download Failed:").borders(Borders::ALL))
                                            .style(Style::default().fg(Color::Gray))
                                            .alignment(Alignment::Center);
                                        f.render_widget(prompt, chunks[0]);
                                        let commands = Paragraph::new(" Back to Results: enter | New Search: s | Quit: q")
                                                .style(Style::default().fg(Color::Gray))
                                                .alignment(Alignment::Left);
                                        f.render_widget(commands, chunks[1]);
                                    }
                                ).unwrap();
                                if poll(Duration::from_millis(100)).unwrap(){
                                    match read().unwrap(){
                                        Event::Key(k) => {
                                            match k.code{
                                                KeyCode::Char(c) if c == 's' || c == 'S' =>{ *loop_state = States::Connected;}
                                                KeyCode::Char(c) if c == 'q' || c == 'Q' =>{return},
                                                KeyCode::Enter => {*loop_state = States::Results;},
                                                _ =>(),
                                            }
                                        },
                                        Event::Mouse(_)=> continue,
                                        Event::Resize(_, _) => continue,
                                    }
                                }},  
            States::SearchFailed =>  {
                                    terminal.draw(
                                        move |f|{
                                            let chunks = Layout::default()
                                            .direction(Direction::Vertical)
                                            .constraints(
                                                [
                                                    Constraint::Percentage(90),
                                                    Constraint::Percentage(10),
                                                ].as_ref()
                                            )
                                            .split(f.size());
                                            let prompt = Paragraph::new("\n\nSearch failed\nEnter to enter a new search.")
                                                .block(Block::default().title("Download Failed:").borders(Borders::ALL))
                                                .style(Style::default().fg(Color::Gray))
                                                .alignment(Alignment::Center);
                                            f.render_widget(prompt, chunks[0]);
                                            let commands = Paragraph::new(" New Search: enter | Quit: q")
                                                    .style(Style::default().fg(Color::Gray))
                                                    .alignment(Alignment::Left);
                                            f.render_widget(commands, chunks[1]);
                                        }
                                    ).unwrap();
                                    if poll(Duration::from_millis(100)).unwrap(){
                                        match read().unwrap(){
                                            Event::Key(k) => {
                                                match k.code{
                                                    KeyCode::Char(c) if c == 'q' || c == 'Q' =>{return},
                                                    KeyCode::Enter => {*loop_state = States::Connected;},
                                                    _ =>(),
                                                }
                                            },
                                            Event::Mouse(_)=> continue,
                                            Event::Resize(_, _) => continue,
                                        }
                                    }},   
            States::Connected => {terminal.draw(
                                    |f|{
                                        let chunks = Layout::default()
                                        .direction(Direction::Vertical)
                                        .constraints(
                                            [
                                                Constraint::Percentage(90),
                                                Constraint::Percentage(10),
                                            ].as_ref()
                                        )
                                        .split(f.size());
                                        let prompt = Paragraph::new(format!["\n\nSearch term: {}", &search_term])
                                            .block(Block::default().title("Search:").borders(Borders::ALL))
                                            .style(Style::default().fg(Color::Gray))
                                            .alignment(Alignment::Center);
                                        f.render_widget(prompt, chunks[0]);
                                        let commands = Paragraph::new(" Search: enter | Quit: ctrl q")
                                                .style(Style::default().fg(Color::Gray))
                                                .alignment(Alignment::Left);
                                        f.render_widget(commands, chunks[1]);
                                    }
                                ).unwrap();
                                if poll(Duration::from_millis(100)).unwrap(){
                                    match read().unwrap(){
                                        Event::Key(k) => {
                                            if k.modifiers.contains(KeyModifiers::CONTROL){
                                                match k.code{
                                                    KeyCode::Char(c) if c == 'q' || c == 'Q' =>{
                                                        return;
                                                    }
                                                    _ => (),
                                                }
                                            }else{
                                                match k.code{
                                                    KeyCode::Char(c) => {search_term.push(c)},
                                                    KeyCode::Backspace => {search_term.pop();},
                                                    KeyCode::Enter => {
                                                        tokio::task::spawn(get_search(
                                                            Args{
                                                                chan: channel.clone(), 
                                                                cmd: format!["{} {}", &search_command, &search_term], 
                                                                client: client.clone(), stream: stream.clone(), 
                                                                items: items.clone(), 
                                                                file_names: file_names.clone(), 
                                                                se: re.clone(), 
                                                                state: state.clone(),
                                                                path: String::new(),
                                                                name: NAME.clone(),
                                                            }
                                                        ));
                                                        *loop_state = States::Search(search_term.clone()); 
                                                        search_term = String::new();},
                                                    _ =>(),
                                                }
                                            }
                                        },
                                        Event::Mouse(_)=> continue,
                                        Event::Resize(_, _) => continue,
                                    }
                                }},
            States::Search(s) =>{
                                terminal.draw(
                                    |f|{
                                        let chunks = Layout::default()
                                            .direction(Direction::Vertical)
                                            .constraints(
                                                [
                                                    Constraint::Percentage(90),
                                                    Constraint::Percentage(10),
                                                ].as_ref()
                                            )
                                            .split(f.size());
                                        let prompt = Paragraph::new(format!["\n\nSearching for: {}\nThis may take a while...", &s])
                                            .block(Block::default().title("Searching:").borders(Borders::ALL))
                                            .style(Style::default().fg(Color::Gray))
                                            .alignment(Alignment::Center);
                                        f.render_widget(prompt, chunks[0]);
                                    }
                                ).unwrap();
                            },
            States::Results=>  {   
                                let mut loop_items = items.lock().await;
                                let selected = if let Some(i) = loop_items.state.selected(){Some(loop_items.items[i].cmd.clone())}else{None};
                                terminal.draw(
                                    move |f|{
                                            let chunks = Layout::default()
                                                .direction(Direction::Vertical)
                                                .constraints(
                                                    [
                                                        Constraint::Percentage(90),
                                                        Constraint::Percentage(10),
                                                    ].as_ref()
                                                )
                                                .split(f.size());
                                            let file_list = List::new(loop_items.items
                                                                        .clone()
                                                                        .iter()
                                                                        .map(|i| i.item.clone()).collect::<Vec<ListItem>>())
                                                .block(Block::default().title("Search Results:").borders(Borders::ALL))
                                                .style(Style::default().fg(Color::Gray))
                                                .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Green))
                                                .highlight_symbol(">>");
                                            f.render_stateful_widget(file_list, chunks[0], &mut loop_items.state);
                                            let commands = Paragraph::new(" Select: arrows | Get: enter | New Search: s | Quit: q")
                                                .style(Style::default().fg(Color::Gray))
                                                .alignment(Alignment::Left);
                                            f.render_widget(commands, chunks[1]);
                                    }
                                ).unwrap();
                                if poll(Duration::from_millis(100)).unwrap(){
                                    match read().unwrap(){
                                        Event::Key(k) => {
                                            match k.code{
                                                KeyCode::Char(c) if c == 's' || c == 'S' =>{ *loop_state = States::Connected;},
                                                KeyCode::Char(c) if c == 'q' || c == 'Q' =>{return},
                                                KeyCode::Down =>{items.lock().await.next();},
                                                KeyCode::Up =>{items.lock().await.previous();},
                                                KeyCode::Enter => { if let Some(s) = selected{*loop_state = States::Get(s);}},
                                                _ =>(),
                                            }
                                        },
                                        Event::Mouse(_)=> continue,
                                        Event::Resize(_, _) => continue,
                                    }
                                }},
            States::Get(cmd) =>{
                                terminal.draw(
                                    |f|{
                                        let chunks = Layout::default()
                                            .direction(Direction::Vertical)
                                            .constraints(
                                                [
                                                    Constraint::Percentage(90),
                                                    Constraint::Percentage(10),
                                                ].as_ref()
                                            )
                                            .split(f.size());
                                        let prompt = Paragraph::new(format!["\n\nDownload book to?\n{}", &dir_path])
                                            .block(Block::default().title("Download:").borders(Borders::ALL))
                                            .style(Style::default().fg(Color::Gray))
                                            .alignment(Alignment::Center);
                                        f.render_widget(prompt, chunks[0]);
                                    }
                                ).unwrap();
                                if poll(Duration::from_millis(100)).unwrap(){
                                    match read().unwrap(){
                                        Event::Key(k) => {
                                            if k.modifiers.contains(KeyModifiers::CONTROL){
                                                match k.code{
                                                    KeyCode::Char(c) if c == 'q' || c == 'Q' =>{
                                                        return;
                                                    }
                                                    _ => (),
                                                }
                                            }else{
                                                match k.code{
                                                    KeyCode::Char(c) => {dir_path.push(c)},
                                                    KeyCode::Backspace => {dir_path.pop();},
                                                    KeyCode::Enter => {
                                                        tokio::task::spawn(get_book(
                                                            Args{
                                                                chan: channel.clone(), 
                                                                cmd: cmd.to_string(), 
                                                                client: client.clone(), 
                                                                stream: stream.clone(), 
                                                                items: items.clone(),
                                                                file_names: file_names.clone(),
                                                                se: re.clone(), 
                                                                state: state.clone(),
                                                                path:dir_path.clone(),
                                                                name: NAME.clone(),
                                                            }
                                                        ));
                                                        *loop_state = States::Getting;
                                                    },
                                                    _ =>(),
                                                }
                                            }
                                        },
                                        Event::Mouse(_)=> continue,
                                        Event::Resize(_, _) => continue,
                                    }
                                }    
                            },
            States::Getting =>{
                terminal.draw(
                    move |f|{
                        let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(
                            [
                                Constraint::Percentage(90),
                                Constraint::Percentage(10),
                            ].as_ref()
                        )
                        .split(f.size());
                        let prompt = Paragraph::new("\n\nYour book is downloading.\nThis may take a while...")
                            .block(Block::default().title("Getting Book:").borders(Borders::ALL))
                            .style(Style::default().fg(Color::Gray))
                            .alignment(Alignment::Center);
                        f.render_widget(prompt, chunks[0]);
                    }
                ).unwrap();
            },
            States::Got =>{
                terminal.draw(
                    move |f|{
                        let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(
                            [
                                Constraint::Percentage(90),
                                Constraint::Percentage(10),
                            ].as_ref()
                        )
                        .split(f.size());
                        let prompt = Paragraph::new("\n\nYour book is downloaded.\nHit enter to select another copy or s to enter a new search.")
                            .block(Block::default().title("Book Downloaded:").borders(Borders::ALL))
                            .style(Style::default().fg(Color::Gray))
                            .alignment(Alignment::Center);
                        f.render_widget(prompt, chunks[0]);
                        let commands = Paragraph::new(" Back to Results: enter | New Search: s | Quit: q")
                                .style(Style::default().fg(Color::Gray))
                                .alignment(Alignment::Left);
                        f.render_widget(commands, chunks[1]);
                    }
                ).unwrap();
                if poll(Duration::from_millis(100)).unwrap(){
                    match read().unwrap(){
                        Event::Key(k) => {
                            match k.code{
                                KeyCode::Char(c) if c == 's' || c == 'S' =>{ *loop_state = States::Connected;}
                                KeyCode::Char(c) if c == 'q' || c == 'Q' =>{return},
                                KeyCode::Enter => {*loop_state = States::Results;},
                                _ =>(),
                            }
                        },
                        Event::Mouse(_)=> continue,
                        Event::Resize(_, _) => continue,
                    }
                }
            },
            _ =>(),
        }
    }
    
}
