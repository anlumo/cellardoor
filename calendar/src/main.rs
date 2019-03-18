use log::{info, debug, error};
use std::io::BufReader;
use std::collections::HashMap;

use chrono::{NaiveDateTime, Local, Duration};
use ical::parser::ical::component::IcalEvent;
use curl::easy::Easy;
use serde_json::json;
use redis::{Commands, PipelineCommands};

const DEFAULT_URL: &'static str = "https://metalab.at/calendar/export/ical/";
const EVENTS_KEY: &'static str = "events";

fn process(event: &IcalEvent) -> Option<(String, String)> {
    let mut startstr = None;
    let mut endstr = None;
    for property in &event.properties {
        match property.name.as_ref() {
            "DTSTART" => {
                if let Some(ref datetime) = property.value {
                    startstr = Some(datetime);
                }
            },
            "DTEND" => {
                if let Some(ref datetime) = property.value {
                    endstr = Some(datetime);
                }
            },
            _ => {}
        }
    }
    if let (Some(startstr), Some(endstr)) = (startstr, endstr) {
        Some((startstr.clone(), endstr.clone()))
    } else {
        None
    }
}

fn event_to_hash_map(event: &IcalEvent) -> HashMap<String, String> {
    event.properties.iter().map(|property| (property.name.clone(), property.value.as_ref().and_then(|val| Some(val.clone())).unwrap_or(String::from("")))).collect()
}

fn main() {
    env_logger::init();

    info!("Fetching calendar...");

    let now = Local::now().naive_local();
    let next_week = now.checked_add_signed(Duration::weeks(1)).unwrap();

    let mut ics = Vec::new();
    let mut easy = Easy::new();
    let url: String = DEFAULT_URL.to_string();

    easy.url(&url).expect("Invalid URL.");
    easy.fail_on_error(true).unwrap();
    {
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            ics.extend_from_slice(data);
            Ok(data.len())
        }).unwrap();
        transfer.perform().unwrap();
    }

    let reader = ical::IcalParser::new(BufReader::new(&*ics));
    let events = match reader.last() {
        Some(Ok(cal)) => {
            cal.events.into_iter().filter_map(|event| {
                if let Some((startstr, endstr)) = process(&event) {
                    if let (Ok(start), Ok(end)) = (NaiveDateTime::parse_from_str(&startstr, "%Y%m%dT%H%M%S"), NaiveDateTime::parse_from_str(&endstr, "%Y%m%dT%H%M%S")) {
                        if now < end && next_week > start {
                            return Some(event_to_hash_map(&event));
                        }
                    }
                }
                None
            }).collect::<Vec<HashMap<String, String>>>()
        },
        Some(Err(err)) => {
            panic!("Parse error: {}", err);
        },
        None => {
            panic!("No calendar found!");
        },
    };

    let client = redis::Client::open("redis://127.0.0.1/").expect("Failed to set up redis client");
    let con = client.get_connection().expect("Failed to connect to redis");
    let events_json = events.iter().filter_map(|event| serde_json::to_string(&json!(event)).ok()).collect::<Vec<String>>();
    if events_json.len() > 0 {
        redis::transaction(&con, &[EVENTS_KEY], |pipe| {
            pipe.del(EVENTS_KEY).sadd(EVENTS_KEY, events_json.clone()).query::<Option<()>>(&con)
        }).expect("Failed executing redis transaction");
    } else {
        info!("No events found.");
        con.del::<_, i32>(EVENTS_KEY).expect("Failed deleting events in redis");
    }
}
