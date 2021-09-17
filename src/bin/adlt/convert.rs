use chrono::{Local, TimeZone};
use slog::{crit, debug, info, warn, error};
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufRead, BufReader, Seek, BufWriter};
use std::sync::mpsc::channel;

enum OutputStyle {
    Hex,
    Ascii,
    Mixed,
    HeaderOnly,
    None,
}

/// same as genivi dlt dlt-convert binary
/// log the files to console
pub fn convert(log: slog::Logger, sub_m: &clap::ArgMatches) -> std::io::Result<()> {
    let input_file_names: Vec<&str> = sub_m.values_of("file").unwrap().collect();

    let output_style: OutputStyle = if sub_m.is_present("hex") {
        OutputStyle::Hex
    } else {
        if sub_m.is_present("ascii") {
            OutputStyle::Ascii
        } else {
            if sub_m.is_present("mixed") {
                OutputStyle::Mixed
            } else {
                if sub_m.is_present("headers") {
                    OutputStyle::HeaderOnly
                } else {
                    OutputStyle::None
                }
            }
        }
    };

    let sort_by_time = sub_m.is_present("sort");

    let index_first: adlt::dlt::DltMessageIndexType = match sub_m.value_of("index_first") {
        None => 0,
        Some(s) => match s.parse::<adlt::dlt::DltMessageIndexType>() {
            Ok(n) => n,
            Err(_) => {
                crit!(log, "index_first '{}' is not a number/index type!", s);
                0 // lets default to 0. could stop as well with u64::MAX
            }
        },
    };
    let index_last: adlt::dlt::DltMessageIndexType = match sub_m.value_of("index_last") {
        None => adlt::dlt::DltMessageIndexType::MAX,
        Some(s) => match s.parse::<adlt::dlt::DltMessageIndexType>() {
            Ok(n) => n,
            Err(_) => {
                crit!(log, "index_last '{}' is not a number/index type!", s);
                0 // let it fail here
            }
        },
    };

    let filter_lc_ids: std::collections::BTreeSet<u32> = match sub_m.values_of("filter_lc_ids") {
        None => std::collections::BTreeSet::new(),
        Some(s) => s
            .map(|s| s.parse::<u32>())
            .filter(|a| !a.is_err())
            .map(|s| s.unwrap())
            .collect(),
    };

    let output_file = match sub_m.value_of("output_file") {
        Some(s) => Some(s.to_string()),
        None => None,
    };

    info!(log, "convert have {} input files", input_file_names.len(); "index_first"=>index_first, "index_last"=>index_last);
    debug!(log, "convert "; "input_file_names" => format!("{:?}",&input_file_names), "filter_lc_ids" => format!("{:?}",filter_lc_ids), "sort_by_time" => sort_by_time);

    // if we have multiple files we do need to sort them first by the first log reception_time!
    if input_file_names.len() > 1 {
        warn!(
            log,
            "input files wont be sorted yet by timestamp but will be read in the order specified!"
        );
    }

    let mut f: Option<BufReader<File>> = None;

    let mut bytes_processed: u64 = 0;
    let mut bytes_per_file: u64 = 0;
    let mut number_messages: adlt::dlt::DltMessageIndexType = 0;
    let mut input_file_names_iter = input_file_names.iter();
    let mut last_data = false;

    // setup (thread) filter chain:
    let (tx, rx) = channel(); // msg -> parse_lifecycles (t2)
    let (tx2, rx2) = channel(); // parse_lifecycles -> buffer_sort_messages (t3)
    // let (tx3, rx3) = channel(); // buffer_sort_messages -> print/output (t4)

    let (lcs_r, lcs_w) =
        evmap::new::<adlt::lifecycle::LifecycleId, adlt::lifecycle::LifecycleItem>();
    let t2 = std::thread::spawn(move || {
        adlt::lifecycle::parse_lifecycles_buffered_from_stream(lcs_w, rx, tx2)
    });
    let t3_lcs_r = lcs_r.clone();
    let (t3, t4_input) = if sort_by_time {
        let (tx3, rx3) = channel();
        (Some(std::thread::spawn(move || {
            adlt::utils::buffer_sort_messages(rx2, tx3, &t3_lcs_r, 3, 2 * adlt::utils::US_PER_SEC)
            /*
            adlt::utils::buffer_elements( // todo we buffer here only to let the lifecycle start times become a bit more stable. need a better way...
                // its only needed to have msgs from different lifecycles sorted as well. within one lifecycle they will be sorted fine
                // the diff is usually just a few (<20) ms...
                rx2,
                tx3,
                adlt::utils::BufferElementsOptions {
                    amount: adlt::utils::BufferElementsAmount::NumberElements(1000), // todo how to determine constant...
                },
            )*/
        })), rx3)
    } else {
        (None, rx2)
    };
    let t4 = std::thread::spawn(move || {
        let mut output_file = if let Some(s) = output_file {
            match std::fs::File::create(s) {
                Ok(f) => Ok(BufWriter::new(f)),
                Err(e) => Err(e),
            }
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "no output_file param",
            ))
        };

        let mut output_screen = std::io::stdout();

        for msg in t4_input {
            // lifecycle filtered?
            if filter_lc_ids.len() > 0 && !filter_lc_ids.contains(&msg.lifecycle) {
                continue;
            }
            // start with a simple dump of the msgs similar to dlt_message_header
            if msg.index >= index_first && msg.index <= index_last {
                // if print header, ascii, hex or mixed: todo
                match output_style {
                    OutputStyle::HeaderOnly => {
                        msg.header_as_text_to_write(&mut output_screen).unwrap(); // todo
                        output_screen.write(&['\n' as u8]).unwrap();
                    }
                    OutputStyle::Ascii => {
                        msg.header_as_text_to_write(&mut output_screen).unwrap(); // todo
                        // output_screen.write(&[' ' as u8]).unwrap();
                        writeln!(output_screen, " [{}]", msg.payload_as_text()); // todo change to write directly to Writer
                        // output_screen.write(&['\n' as u8]).unwrap();
                    }
                    _ => {
                        // todo...
                    }
                }
                // if output to file:
                if let Ok(ref mut file) = output_file {
                    msg.to_write(file).unwrap(); // todo err handling
                }
            }
        }
        if output_file.is_ok() {
            let mut writer = output_file.unwrap();
            writer.flush().unwrap();
            drop(writer); // close, happens anyhow autom...
        }
    });

    loop {
        if f.is_none() {
            // load next file
            let input_file_name = input_file_names_iter.next();
            match input_file_name {
                None => {
                    break;
                }
                Some(input_file_name) => {
                    let fi = File::open(input_file_name)?;
                    info!(log, "opened file {} {:?}", &input_file_name, &fi);
                    const BUFREADER_CAPACITY: usize = 0x10000 * 50;
                    f = Some(BufReader::with_capacity(BUFREADER_CAPACITY, fi));
                }
            }
        }
        assert!(!f.is_none());
        let reader: &mut BufReader<File> = f.as_mut().unwrap();
        match adlt::dlt::parse_dlt_with_storage_header(number_messages, &mut *reader) {
            Ok((res, msg)) => {
                bytes_per_file += res as u64;
                number_messages += 1;

                tx.send(msg).unwrap(); // todo handle error?

                // get more data from BufReader:
                if !last_data && (reader.buffer().len() < 0x10000) {
                    // read more data
                    let pos = std::io::SeekFrom::Start(bytes_per_file);
                    reader.seek(pos).unwrap();
                    reader.fill_buf().expect("fill_buf 2 failed!");
                    last_data = reader.buffer().len() < 0x10000;
                }
            }
            Err(error) => match error.kind() {
                adlt::dlt::ErrorKind::InvalidData(_str) => {
                    bytes_per_file += 1;
                    reader.consume(1); // skip 1 byte and check for new valid storage_header
                    info!(log, "skipped 1 byte at {}", bytes_per_file);
                }
                _ => {
                    debug!(log, "finished processing a file"; "bytes_per_file"=>bytes_per_file, "number_messages"=>number_messages);
                    f.unwrap();
                    f = None; // check for next file on next it
                    last_data = false;
                    bytes_processed += bytes_per_file;
                    bytes_per_file = 0;
                    info!(log, "got Error {}", error);
                }
            },
        }
    }
    drop(tx);
    let _lcs_w = t2.join().unwrap();

    if let Some(t) = t3 {
        match t.join() {
            Err(s) => error!(log, "t3 join got Error {:?}", s),
            Ok(s) => debug!(log, "t3 join was Ok {:?}", s),
        };
    }

    t4.join().unwrap();

    info!(log, "finished processing"; "bytes_processed"=>bytes_processed, "number_messages"=>number_messages);

    // print lifecycles:
    if let Some(a) = lcs_r.read() {
        let sorted_lcs = adlt::lifecycle::get_sorted_lifecycles_as_vec(&a);
        info!(log, "have {} lifecycles:", sorted_lcs.len(),);
        // output lifecycles
        for lc in sorted_lcs {
            info!(
                log,
                "LC#{:3}: {:4} {} - {} #{:8} {}",
                lc.id(),
                lc.ecu,
                Local
                    .from_utc_datetime(&adlt::utils::utc_time_from_us(lc.start_time))
                    .format("%Y/%m/%d %H:%M:%S%.6f"),
                Local
                    .from_utc_datetime(&adlt::utils::utc_time_from_us(lc.end_time()))
                    .format("%H:%M:%S"),
                lc.nr_msgs,
                if lc.only_control_requests() {
                    "CTRL_REQUESTS_ONLY"
                } else {
                    ""
                }
            );
        }
    }

    Ok(())
}