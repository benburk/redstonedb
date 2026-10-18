-- A hash of the circuit a result ran on, alone and stacked each way the
-- build stacks, so a new file holding the same circuit keeps the result
-- rather than being simulated again. NULL for results from before.
ALTER TABLE simulation ADD COLUMN circuit TEXT;
