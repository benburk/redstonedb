//! Probing for a decoder: its one input line A and its one output line, a
//! bit for each of A's values, coloured and run through the decoder test.

use std::collections::HashMap;

use super::{Fault, lengths, try_roles};
use crate::{
    recognize::inspect,
    schem::Pos,
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
) -> Result<DecoderProbed, Fault> {
    let [a] = report.inputs.as_slice() else {
        return Err(Fault::Unfit(format!(
            "Input: line lengths {}, need one line A",
            lengths(&report.inputs)
        )));
    };
    let [out] = report.outputs.as_slice() else {
        return Err(Fault::Unfit(format!(
            "Output: line lengths {}, need one line",
            lengths(&report.outputs)
        )));
    };
    if out.bits.len() != 1 << a.bits.len().min(16) {
        return Err(Fault::Unfit(format!(
            "Output: {} bits, need one for each of A's {} values",
            out.bits.len(),
            1_usize << a.bits.len().min(16)
        )));
    }
    let mut roles: HashMap<Pos, &str> = HashMap::new();
    roles.extend(a.bits.iter().map(|&p| (p, "lime")));
    roles.extend(out.bits.iter().map(|&p| (p, "yellow")));
    let (file, run) = try_roles(data, &roles, Test::Decoder, false)??;
    Ok(DecoderProbed {
        a: a.bits.clone(),
        output: out.bits.clone(),
        file,
        run,
    })
}
