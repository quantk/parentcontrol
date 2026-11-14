-- Schema version 2. Bonuses become signed adjustments: the parent can take
-- time away as well as add it. Existing rows keep their positive minutes.

ALTER TABLE bonuses RENAME TO adjustments;
