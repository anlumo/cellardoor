use log::{info, debug, error};
use curl::easy::Easy;
use redis::{Commands, PipelineCommands};
use std::io::{BufReader, BufRead};

const DEFAULT_URL: &'static str = "<RETRACTED>";
const IBUTTONS_KEY: &'static str = "ibuttons";

fn main() {
    env_logger::init();
    info!("Fetching iButtons...");

    let mut ibuttons = Vec::new();
    let mut easy = Easy::new();
    let url: String = DEFAULT_URL.to_string();

    easy.url(&url).expect("Invalid URL.");
    easy.fail_on_error(true).unwrap();
    {
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            ibuttons.extend_from_slice(data);
            Ok(data.len())
        }).unwrap();
        transfer.perform().unwrap();
    }

    let reader = BufReader::new(&*ibuttons);

    let ids = reader.lines().filter_map(|id| id.ok()).collect::<Vec<String>>();
    debug!("ids: {:?}", ids);

    let client = redis::Client::open("redis://127.0.0.1/").expect("Failed to set up redis client");
    let con = client.get_connection().expect("Failed to connect to redis");
    if ids.len() > 0 {
        redis::transaction(&con, &[IBUTTONS_KEY], |pipe| {
            pipe.del(IBUTTONS_KEY).sadd(IBUTTONS_KEY, ids.clone()).query::<Option<()>>(&con)
        }).expect("Failed executing redis transaction");
    } else {
        info!("No iButtons found.");
        con.del::<_, i32>(IBUTTONS_KEY).expect("Failed deleting iButtons in redis");
    }
}
