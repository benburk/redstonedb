-- A register's stacking search now also tries copies that add bits,
-- sharing the control lines, so its results run again.
DELETE FROM simulation WHERE test = 'register';
