use tui::widgets::{ListItem, ListState};
use tokio::sync::{mpsc, Mutex};
use std::sync::Arc;
use regex::Regex;

pub struct Args<'a>{
    pub chan: Arc<String>, 
    pub cmd: String, 
    pub client: Arc<irc::client::Client>, 
    pub stream: Arc<Mutex<mpsc::UnboundedReceiver<Result<irc::proto::Message, irc::error::Error>>>>, 
    pub items: Arc<Mutex<StateList<Item<'a>>>>, 
    pub file_names: Arc<Mutex<Vec<String>>>, 
    pub se: Regex, 
    pub state: Arc<Mutex<States>>,
    pub path: String
}
#[derive(Clone)]
pub struct StateList<T> {
    pub state: ListState,
    pub items: Vec<T>,
}
impl<T> StateList<T>{
    pub fn new() ->StateList<T>{
        StateList{
            state: ListState::default(),
            items: Vec::new(),
        }
    }
    pub fn from(items: Vec<T>) -> StateList<T>{
        let mut s = StateList{
            state: ListState::default(),
            items,
        };
        s.state.select(Some(0));
        s
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    self.items.len()-1
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    0
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }
}
pub enum States{
    Connect,
    Connecting(String),
    Connected,
    Failed,
    SearchFailed,
    Search(String),
    Results,
    Get(String),
    Getting,
    Got,
}
#[derive(Clone)]
pub struct Item<'a>{
    pub item: ListItem<'a>,
    pub cmd: String,
}
