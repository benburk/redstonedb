CREATE TABLE invite_code(
    code          TEXT NOT NULL PRIMARY KEY
                  CHECK (
                      length(code) = 16
                      AND code = upper(code)
                      AND code NOT GLOB '*[^0123456789ABCDEFGHJKMNPQRSTVWXYZ]*'
                  ),
    issued_by     BLOB NOT NULL REFERENCES user(id) ON DELETE RESTRICT,
    redeemed_by   BLOB UNIQUE REFERENCES user(id) ON DELETE RESTRICT,
    created_at    INTEGER NOT NULL,
    redeemed_at   INTEGER,
    CHECK (
        (redeemed_by IS NULL AND redeemed_at IS NULL)
        OR
        (redeemed_by IS NOT NULL AND redeemed_at IS NOT NULL AND redeemed_at >= created_at)
    )
) STRICT;

CREATE INDEX invite_code_issued_by_idx ON invite_code(issued_by);
