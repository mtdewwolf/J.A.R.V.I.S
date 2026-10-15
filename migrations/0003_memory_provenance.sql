-- Track transitive memory use so deletion also erases derived conversations.
ALTER TABLE turns ADD COLUMN memory_refs TEXT[] NOT NULL DEFAULT '{}';
