-- Add migration script here
ALTER TABLE history
    ADD search_date TIMESTAMP NOT NULL DEFAULT '1-1-0 00:00:00';