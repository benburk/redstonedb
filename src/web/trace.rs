//! A build's logic analyzer: one change of its inputs, as the query names
//! it, traced tick by tick, each way its races may go, and a sweep of many
//! changes, for how its outputs settle, where they glitch, and its races.
//! See `simulate::trace`.

use std::time::{Duration, Instant};

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};

use crate::{
    category::Kind,
    models::{schematic, user::User, version},
    recognize::{inspect::place, interface::Interface},
    schem::{self, Blocks, Direction, Pos},
    simulate::{
        Value,
        trace::{self, Case, Change, Graph, Part, Race, Sweep, Trace, Tracer},
    },
    web::{app_state::AppState, error::AppError},
};

/// The longest a sweep runs, so a large build's page still answers.
const SWEEP: Duration = Duration::from_secs(3);

/// Changes swept at random, besides each input alone from all off.
const RANDOM: usize = 1000;

/// The most times the change is run again, compiled afresh, for its race
/// to go the other way.
const TRIES: usize = 12;

/// One of a build's lines, as the page names and draws it.
pub struct Line {
    pub name: String,
    pub width: usize,
    /// Each bit's block, least significant first.
    pub blocks: Vec<Pos>,
}

impl Line {
    /// Bit `bit`'s name, such as `B0`, `output bit 3`, or a lone bit's line
    /// name, such as `carry in`.
    pub fn bit_name(&self, bit: usize) -> String {
        if self.width == 1 {
            self.name.clone()
        } else if self.name.chars().count() <= 2 {
            format!("{}{bit}", self.name)
        } else {
            format!("{} bit {bit}", self.name)
        }
    }
}

/// One run of the change: its trace and its races.
pub struct Run {
    pub trace: Trace,
    pub races: Vec<Race>,
}

/// What the page shows.
pub struct Analysis {
    pub inputs: Vec<Line>,
    pub outputs: Vec<Line>,
    pub graph: Graph,
    /// Each input's lever, by bit, and what reads each output bit.
    pub levers: Vec<(Pos, usize, usize)>,
    pub readers: Vec<(Pos, usize, usize)>,
    /// The change traced, and the tick each input switches on.
    pub case: Case,
    pub at: Vec<i32>,
    /// The change as run, then, when its first race went the other way in
    /// another compile, that run too.
    pub runs: Vec<Run>,
    pub sweep: Sweep,
    /// How many changes the sweep ran at random.
    pub random: usize,
}

impl Analysis {
    /// Node `node`'s name and place, such as `torch at (9, 3, 4)`, a lever
    /// by the bit it switches and what reads an output by the bit it reads.
    pub fn label(&self, node: usize) -> String {
        let n = &self.graph.nodes[node];
        let lever = self.levers.iter().find(|(at, ..)| *at == n.at);
        if let Some(&(_, port, bit)) = lever {
            return format!(
                "{} for {} at {}",
                n.part.name(),
                self.inputs[port].bit_name(bit),
                place(n.at)
            );
        }
        let reader = self.readers.iter().find(|(at, ..)| *at == n.at);
        if let Some(&(_, port, bit)) = reader {
            return format!(
                "{} at {} reading {}",
                n.part.name(),
                place(n.at),
                self.outputs[port].bit_name(bit)
            );
        }
        format!("{} at {}", n.part.name(), place(n.at))
    }
}

/// The logic analyzer for build `id`.
pub async fn page(
    Path(id): Path<String>,
    State(state): State<AppState>,
    viewer: Option<User>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Response, AppError> {
    let db = &state.db;
    let s = schematic::get_visible(db, &id, viewer.as_ref())
        .await?
        .ok_or(AppError::NotFound)?;
    let data = version::current_file(db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    let kind = Kind::named(&s.category);
    let analysis =
        tokio::task::spawn_blocking(move || analyze(&data, kind, &pairs))
            .await
            .map_err(anyhow::Error::from)?;
    Ok(
        super::pages::trace_page(viewer.as_ref(), &s, analysis.as_ref())
            .into_response(),
    )
}

/// The build in `data`, carrying its roles, of `kind`, traced through the
/// change `pairs` names, as `from.A=3`, `to.B=1`, and `at.A=1`, or, when
/// they name none, the change its sweep found most worth seeing; or why it
/// could not be.
pub fn analyze(
    data: &[u8],
    kind: Option<Kind>,
    pairs: &[(String, String)],
) -> Result<Analysis, String> {
    let blocks = Blocks::read(data).map_err(|err| err.to_string())?;
    let interface = lines(&blocks, kind).map_err(|err| err.to_string())?;
    let inputs = described(&interface, true);
    let outputs = described(&interface, false);
    let levers = levers(&interface);
    let readers = readers(&interface);
    let mut tracer =
        Tracer::new(&blocks, interface).map_err(|err| err.to_string())?;
    let widths = tracer.input_widths();
    let cases = trace::cases(&widths, RANDOM);
    let sweep = trace::sweep(&mut tracer, &cases, Some(Instant::now() + SWEEP))
        .map_err(|err| err.to_string())?;
    let (case, at) = match chosen(&inputs, pairs)? {
        Some(chosen) => chosen,
        None => (worth_seeing(&sweep, &widths), vec![0; widths.len()]),
    };
    let mut runs = Vec::new();
    let first = run(&mut tracer, &case, &at)?;
    let goes = |run: &Run| run.races.first().map(|race| (race.node, race.ran));
    let seen = goes(&first);
    runs.push(first);
    // The first race may go the other way in a compile of its own.
    if let Some(seen) = seen {
        for _ in 0..TRIES {
            let mut again = Tracer::new(
                &blocks,
                lines(&blocks, kind).map_err(|e| e.to_string())?,
            )
            .map_err(|err| err.to_string())?;
            let other = run(&mut again, &case, &at)?;
            if goes(&other).is_some_and(|went| went != seen) {
                runs.push(other);
                break;
            }
        }
    }
    Ok(Analysis {
        inputs,
        outputs,
        graph: tracer.graph,
        levers,
        readers,
        case,
        at,
        runs,
        sweep,
        random: RANDOM,
    })
}

/// The build's lines, named as its category's test names them.
fn lines(blocks: &Blocks, kind: Option<Kind>) -> anyhow::Result<Interface> {
    match kind.and_then(Kind::test) {
        Some(test) => test.lines(blocks),
        None => Ok(crate::recognize::interface::read(blocks)),
    }
}

fn run(tracer: &mut Tracer, case: &Case, at: &[i32]) -> Result<Run, String> {
    let trace = tracer
        .run(&case.from, &case.to, at)
        .map_err(|err| err.to_string())?;
    let races = trace.races(&tracer.graph);
    Ok(Run { trace, races })
}

/// The inputs, or the outputs, of `interface` as lines.
fn described(interface: &Interface, inputs: bool) -> Vec<Line> {
    let ports = if inputs {
        &interface.inputs
    } else {
        &interface.outputs
    };
    ports
        .iter()
        .map(|port| Line {
            name: port.name.clone(),
            width: if port.signal_strength {
                4 * port.bits.len()
            } else {
                port.bits.len()
            },
            blocks: port.bits.iter().map(|bit| bit.pos).collect(),
        })
        .collect()
}

/// Each input bit's lever, with its input and bit.
fn levers(interface: &Interface) -> Vec<(Pos, usize, usize)> {
    interface
        .inputs
        .iter()
        .enumerate()
        .flat_map(|(port, line)| {
            line.bits.iter().enumerate().filter_map(move |(bit, b)| {
                Some((schem::step(b.pos, b.entry?), port, bit))
            })
        })
        .collect()
}

/// What stands on each output bit's block, reading it, with its output and
/// bit.
fn readers(interface: &Interface) -> Vec<(Pos, usize, usize)> {
    interface
        .outputs
        .iter()
        .enumerate()
        .flat_map(|(port, line)| {
            line.bits.iter().enumerate().map(move |(bit, b)| {
                (schem::step(b.pos, Direction::Up), port, bit)
            })
        })
        .collect()
}

/// The change `pairs` names, as `from.A=3`, `to.B=1`, and `at.A=1`, an
/// input's name after the dot: any input not named changes from 0 to its
/// value before, switching at tick 0. `None` when they name no input.
fn chosen(
    inputs: &[Line],
    pairs: &[(String, String)],
) -> Result<Option<(Case, Vec<i32>)>, String> {
    let n = inputs.len();
    let mut from: Vec<Option<Value>> = vec![None; n];
    let mut to: Vec<Option<Value>> = vec![None; n];
    let mut at = vec![0; n];
    let mut any = false;
    for (key, value) in pairs {
        let Some((side, name)) = key.split_once('.') else {
            continue;
        };
        let value = value.trim();
        let Some(port) = inputs
            .iter()
            .position(|line| line.name.eq_ignore_ascii_case(name.trim()))
        else {
            return Err(format!("The build has no input named {name}."));
        };
        any = true;
        match side {
            "from" | "to" => {
                if value.is_empty() {
                    continue;
                }
                let parsed = parse_value(value).ok_or_else(|| {
                    format!("{value} is not a value for {name}.")
                })?;
                let width = inputs[port].width;
                if width < 128 && parsed >> width != 0 {
                    return Err(format!(
                        "{name} is {width} bits wide, too few for {value}."
                    ));
                }
                if side == "from" {
                    from[port] = Some(parsed);
                } else {
                    to[port] = Some(parsed);
                }
            }
            "at" => {
                if value.is_empty() {
                    continue;
                }
                at[port] = value
                    .parse::<i32>()
                    .ok()
                    .filter(|tick| (0..=100).contains(tick))
                    .ok_or_else(|| {
                        format!("{name} switches at a tick from 0 to 100, not {value}.")
                    })?;
            }
            _ => {}
        }
    }
    if !any {
        return Ok(None);
    }
    let from: Vec<Value> = from.into_iter().map(|v| v.unwrap_or(0)).collect();
    let to: Vec<Value> = to
        .into_iter()
        .zip(&from)
        .map(|(to, &from)| to.unwrap_or(from))
        .collect();
    Ok(Some((Case { from, to }, at)))
}

/// A value written in decimal, or in hex after `0x`, or binary after `0b`.
fn parse_value(text: &str) -> Option<Value> {
    let text = text.replace('_', "");
    if let Some(hex) = text.strip_prefix("0x") {
        Value::from_str_radix(hex, 16).ok()
    } else if let Some(binary) = text.strip_prefix("0b") {
        Value::from_str_radix(binary, 2).ok()
    } else {
        text.parse().ok()
    }
}

/// The change most worth seeing: the first race's, else the slowest
/// change of an output, else every input of the first turning on.
fn worth_seeing(sweep: &Sweep, widths: &[usize]) -> Case {
    if let Some(seen) = sweep.races.iter().max_by_key(|seen| seen.count) {
        return seen.case.clone();
    }
    if let Some((_, case)) = sweep
        .outputs
        .iter()
        .filter_map(|output| output.slowest.as_ref())
        .max_by_key(|(tick, _)| *tick)
    {
        return case.clone();
    }
    let mut to = vec![0; widths.len()];
    if let Some(first) = to.first_mut() {
        *first = crate::simulate::mask(widths[0]);
    }
    Case {
        from: vec![0; widths.len()],
        to,
    }
}

/// The change in words, such as `A=0, B=0, carry in=0 → B=1`.
pub fn case_words(a: &Analysis, case: &Case) -> String {
    let from: Vec<String> = a
        .inputs
        .iter()
        .zip(&case.from)
        .map(|(line, value)| format!("{}={value}", line.name))
        .collect();
    let to: Vec<String> = a
        .inputs
        .iter()
        .zip(case.from.iter().zip(&case.to))
        .filter(|(_, (from, to))| from != to)
        .map(|(line, (_, to))| format!("{}={to}", line.name))
        .collect();
    if to.is_empty() {
        format!("{}, changing nothing", from.join(", "))
    } else {
        format!("{} → {}", from.join(", "), to.join(", "))
    }
}

/// What a part giving out `strength` is, such as `lit` or `13`.
pub fn state(part: Part, strength: u8) -> String {
    match part {
        Part::Torch if strength > 0 => "lit".to_owned(),
        Part::Torch => "out".to_owned(),
        Part::Comparator { .. } => strength.to_string(),
        _ if strength > 0 => "on".to_owned(),
        _ => "off".to_owned(),
    }
}

/// A change in words, as `lights`, or with `ing` as `lighting`.
pub fn verb(part: Part, change: &Change, ing: bool) -> String {
    let on = change.now > 0;
    let words = match part {
        Part::Torch => {
            if on {
                ["lights", "lighting"]
            } else {
                ["goes out", "going out"]
            }
        }
        Part::Comparator { .. } => {
            let to = if ing { "going" } else { "goes" };
            return format!("{to} from {} to {}", change.was, change.now);
        }
        Part::Switch { .. } => {
            if on {
                ["switches on", "switched on"]
            } else {
                ["switches off", "switched off"]
            }
        }
        _ => {
            if on {
                ["turns on", "turning on"]
            } else {
                ["turns off", "turning off"]
            }
        }
    };
    words[usize::from(ing)].to_owned()
}

/// What a part does going from `was` to `now`, such as `lights` or
/// `stays out`.
pub fn becomes(part: Part, was: u8, now: u8) -> String {
    if was == now {
        return match part {
            Part::Comparator { .. } => format!("stays at {now}"),
            _ => format!("stays {}", state(part, now)),
        };
    }
    match part {
        Part::Torch if now > 0 => "lights".to_owned(),
        Part::Torch => "goes out".to_owned(),
        Part::Comparator { .. } => format!("goes to {now}"),
        _ if now > 0 => "turns on".to_owned(),
        _ => "turns off".to_owned(),
    }
}
