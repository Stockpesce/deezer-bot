-- Add migration script here
ALTER TABLE history ALTER search_date TYPE timestamptz USING search_date AT TIME ZONE 'UTC';
