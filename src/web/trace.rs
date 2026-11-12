//! A build's traced glitch: the change of its inputs its timing found an
//! output glitching on, run tick by tick, for the chart in its Timing
//! section. See `simulate::trace`.

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use maud::html;

use crate::{
    category::Kind,
    interface::Interface,
    models::simulation,
    schem::Blocks,
    simulate::{
        Glitch, OutputTiming, Timing, Value,
        trace::{Case, Race, Trace, Tracer},
    },
    store::{builds, versions},
    web::{app_state::AppState, error::AppError},
};

/// The most times the change is run again, compiled afresh, for its
/// output to glitch as its timing found it to.
const TRIES: usize = 12;

/// One of a build's lines, as the chart names and draws it.
pub struct Line {
    pub name: String,
    pub width: usize,
}

/// One run of the change: its trace and its races.
pub struct Run {
    pub trace: Trace,
    pub races: Vec<Race>,
}

/// What the chart shows.
pub struct Analysis {
    pub inputs: Vec<Line>,
    pub outputs: Vec<Line>,
    /// The change traced, and the tick each input switches on.
    pub case: Case,
    pub at: Vec<i32>,
    /// The change as Redpiler ran it.
    pub run: Run,
    /// The change run block by block, its races going the way the game's
    /// block updates take them; see [`Tracer::in_world`].
    pub world: Option<Run>,
}

/// A build set up to be traced: its blocks, its kind, and its lines, as the
/// chart names and draws them.
struct Setup {
    blocks: Blocks,
    kind: Option<Kind>,
    inputs: Vec<Line>,
    outputs: Vec<Line>,
}

impl Setup {
    /// The build in `data`, carrying its roles, of `kind`.
    fn new(data: &[u8], kind: Option<Kind>) -> Result<Setup, String> {
        let blocks = Blocks::read(data).map_err(|err| err.to_string())?;
        let interface = lines(&blocks, kind).map_err(|err| err.to_string())?;
        Ok(Setup {
            inputs: described(&interface, true),
            outputs: described(&interface, false),
            blocks,
            kind,
        })
    }

    /// The build compiled afresh, which may run its races its own way.
    fn tracer(&self) -> Result<Tracer, String> {
        let interface =
            lines(&self.blocks, self.kind).map_err(|err| err.to_string())?;
        Tracer::new(&self.blocks, interface).map_err(|err| err.to_string())
    }

    /// `case`, each input switching on its tick of `at`, run block by
    /// block; `None` when the build cannot run so.
    fn world(&self, case: &Case, at: &[i32]) -> Option<Run> {
        let interface = lines(&self.blocks, self.kind).ok()?;
        let mut tracer = Tracer::in_world(&self.blocks, interface).ok()?;
        run(&mut tracer, case, at).ok()
    }
}

/// The output whose glitch a build's timing shows traced: its first data
/// output to glitch, else its first status line to.
pub fn shown_glitch(timing: &Timing) -> Option<&OutputTiming> {
    let glitched = || timing.outputs.iter().filter(|o| o.glitch.is_some());
    glitched().find(|o| !o.status).or_else(|| glitched().next())
}

/// The change `glitch`, of the build in `data`, carrying its roles, of
/// `kind`, each input switching on its tick of `timing`, as the timing
/// that found it switched them: run on fresh compiles until output
/// `output` glitches as it did, or, if it never does, as first run.
fn glitch(
    data: &[u8],
    kind: Option<Kind>,
    timing: &Timing,
    output: &str,
    glitch: &Glitch,
) -> Result<Analysis, String> {
    let setup = Setup::new(data, kind)?;
    let port = setup
        .outputs
        .iter()
        .position(|line| line.name == output)
        .ok_or_else(|| format!("The build has no output named {output}."))?;
    let by_name = |line: &Line| {
        timing
            .inputs
            .iter()
            .position(|input| input.name == line.name)
    };
    let found: Vec<Option<usize>> = setup.inputs.iter().map(by_name).collect();
    let value = |values: &[Value], i: Option<usize>| {
        i.and_then(|i| values.get(i).copied()).unwrap_or(0)
    };
    let to: Vec<Value> =
        found.iter().map(|&i| value(&glitch.case, i)).collect();
    // From all off, unless it came from other values.
    let from: Vec<Value> =
        found.iter().map(|&i| value(&glitch.from, i)).collect();
    let at: Vec<i32> = found
        .iter()
        .map(|&i| i.and_then(|i| timing.inputs[i].tick).unwrap_or(0))
        .collect();
    let case = Case { from, to };
    let glitched = |run: &Run| {
        (0..setup.outputs[port].width)
            .any(|bit| run.trace.bit_changes(port, bit).len() > 1)
    };
    let mut shown = run(&mut setup.tracer()?, &case, &at)?;
    // A change kept for no glitch, as the slowest, is shown as first run.
    let tries = if glitch.steps.is_empty() { 0 } else { TRIES };
    for _ in 0..tries {
        if glitched(&shown) {
            break;
        }
        let other = run(&mut setup.tracer()?, &case, &at)?;
        if glitched(&other) {
            shown = other;
        }
    }
    let world = setup.world(&case, &at);
    Ok(Analysis {
        inputs: setup.inputs,
        outputs: setup.outputs,
        case,
        at,
        run: shown,
        world,
    })
}

/// The traced glitch of build `id`'s timing, to show in its Timing
/// section; nothing for a build whose timing has none.
pub async fn glitch_fragment(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let db = &state.db;
    let s = builds::get(db, &id).await?.ok_or(AppError::NotFound)?;
    let report = simulation::current(db, &s).await?;
    let Some(timing) = report.as_ref().and_then(|r| r.timing()).cloned() else {
        return Ok(html! {}.into_response());
    };
    // Its glitch when it has one; else its slowest change from all off,
    // so its timing always shows a change as it runs.
    let (title, output, shown) = if let Some(output) = shown_glitch(&timing) {
        (
            "Glitch",
            output.name.clone(),
            output.glitch.clone().expect("a shown glitch"),
        )
    } else {
        let Some(output) = timing
            .outputs
            .iter()
            .find(|o| !o.status)
            .or(timing.outputs.first())
        else {
            return Ok(html! {}.into_response());
        };
        let slowest = Glitch {
            from: Vec::new(),
            case: timing.slowest_case.clone(),
            before: 0,
            steps: Vec::new(),
        };
        ("Slowest change", output.name.clone(), slowest)
    };
    let data = versions::current_file(db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    let kind = Kind::named(&s.category);
    let name = output.clone();
    let analysis = tokio::task::spawn_blocking(move || {
        glitch(&data, kind, &timing, &name, &shown)
    })
    .await
    .map_err(anyhow::Error::from)?;
    Ok(match analysis {
        Ok(analysis) => super::pages::glitch_trace(&analysis, &output, title)
            .into_response(),
        Err(why) => html! {
            p class="mt-4 text-muted" { "Could not trace its glitch: " (why) }
        }
        .into_response(),
    })
}

/// The build's lines, named as its category's test names them.
fn lines(blocks: &Blocks, kind: Option<Kind>) -> anyhow::Result<Interface> {
    match kind.and_then(Kind::test) {
        Some(test) => test.lines(blocks),
        None => Ok(crate::interface::read(blocks)),
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
        })
        .collect()
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
