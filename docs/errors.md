# Error messages

Every error the site shows a person, on a build's checks, the upload page, a form, or a refused request, follows one shape: the problem first, short, with no rule restated. The fix, and the rule behind it, go in the advice beside it and the Build Analysis page it links to.

## Shapes

- **Expected X, found Y**, when there is a definite expectation: `Expected 2 operands, found 3`, `Expected 1 carry in, found 2 control lines`, `Expected output width 8, found 9`.
- **Too many / missing**, when there is no single number to expect: `Too many control lines`, `Missing carry out`, `Missing outputs`.
- **Thing: problem**, for a fault at one place in the build: `Lever on bottom of block`, `Input drives nothing`, `Repeater: nothing behind it`.
- **A plain statement**, for anything else, as short as it can be: `Never comes to rest`, `Took too long to simulate`, `File can't be read`.

## Rules

- Start with the problem. No `A … takes …, but this has …`: that states the rule, then contradicts it.
- Leave out the category; the page already names it. `Expected 2 operands, found 3`, not `An adder takes two operands, but this has 3`.
- Name lines as the page does: `A`, `B`, `carry in`, `carry out`, `output`, `control line`, `status line`.
- Use digits for counts: `Expected 2 operands`, not `two operands`.
- Say a width as `width 8`, not `8 bits`, so a 1-bit build doesn't read `1 bits`: `Expected output width 1, found 3`.
- One fact a message. Two problems are two messages.
- No closing period on a phrase; a message of more than one sentence is a sign it should be shorter.
- Plain words: `such lines`, `this one`, and `as need be` make a reader look back.
- A wrong case in a test keeps its own shape, the inputs and what came out against what was expected: `A=128 B=0 → output=0b11111111, expected 0b00000000`.
