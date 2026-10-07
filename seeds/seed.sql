INSERT INTO user (id, username, password_hash, created_at)
VALUES (
    X'FAAF4E0603194B2A999A3B0F5BBE0577',
    'admin',
    '$argon2id$v=19$m=19456,t=2,p=1$mL9KGD0cIvZEHMP0GilUEw$jBnS2AJEOyuLCSSanhmdcrl30pGTaD8qigDgsXuQFek',
    1758306817248843
);

-- 16 Crockford Base32 characters carry 80 bits of entropy while remaining
-- easy to read and type. Applications may display these in groups of four.
WITH codes(code) AS (
    VALUES
        ('SXNHWA6953JC034X'),
        ('0416F9YG33B92AA4'),
        ('2X8SKTZG8SSRXHHJ'),
        ('M4BSZXWQ0NSK87MS'),
        ('C96BKHMRTXVGZC20'),
        ('STRZ6Q1CW9AYBJPK'),
        ('7X6KBJEA3K25ZV81'),
        ('Q8ARE24N1M6TV2W2'),
        ('ETFEZN7R7HJY06FT'),
        ('TVQQSZW2KVT072JR')
)
INSERT INTO invite_code (code, issued_by, created_at)
SELECT code, id, 1758306817248843
FROM user
CROSS JOIN codes
WHERE username = 'admin';
