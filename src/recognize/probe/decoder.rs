//! Probing for a decoder: its one input line A and its one output line, a
//! bit for each of A's values, coloured and run through the decoder test.

use std::collections::HashMap;

use super::{lengths, paint, recognize::doesnt_hold};
use crate::{
    recognize::inspect,
    schem::{Blocks, Pos},
    simulate::{self, Test},
};

/// A decoder as probing found it: its input A, its output, the file with
/// them coloured as the decoder test reads them, and that test's run.
pub struct DecoderProbed {
    pub a: Vec<Pos>,
    pub output: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The decoder the file in `data` is, from `report`'s levers and outputs:
/// its one input line A and its one output line, a bit for each of A's
/// values, coloured and run through the decoder test. Or why it isn't.
pub fn decoder(
    data: &[u8],
    report: &inspect::Report,
) -> Result<DecoderProbed, String> {
    let [a] = report.inputs.as_slice() else {
        return Err(format!(
            "Input: line lengths {}, need one line A",
            lengths(&report.inputs)
        ));
    };
    let [out] = report.outputs.as_slice() else {
        return Err(format!(
            "Output: line lengths {}, need one line",
            lengths(&report.outputs)
        ));
    };
    if out.bits.len() != 1 << a.bits.len().min(16) {
        return Err(format!(
            "Output: {} bits, need one for each of A's {} values",
            out.bits.len(),
            1_usize << a.bits.len().min(16)
        ));
    }
    let mut roles: HashMap<Pos, &str> = HashMap::new();
    roles.extend(a.bits.iter().map(|&p| (p, "lime")));
    roles.extend(out.bits.iter().map(|&p| (p, "yellow")));
    let file = paint(data, &roles).map_err(|err| err.to_string())?;
    let coloured = Blocks::read(&file).map_err(|err| err.to_string())?;
    match Test::Decoder.run(&coloured) {
        simulate::Report::Ran(run) if run.wrong == 0 => Ok(DecoderProbed {
            a: a.bits.clone(),
            output: out.bits.clone(),
            file,
            run: *run,
        }),
        simulate::Report::Ran(run) => Err(doesnt_hold(&run, &file)),
        simulate::Report::Broken { reason }
        | simulate::Report::Skipped { reason } => {
            Err(format!("Test couldn't run: {reason}"))
        }
    }
}
