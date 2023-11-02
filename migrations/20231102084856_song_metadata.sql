-- Add migration script here
ALTER TABLE songs
    ADD song_name   VARCHAR(255) NOT NULL DEFAULT 'Unknown song',
    ADD song_artist VARCHAR(255) NOT NULL DEFAULT 'Unknown artist';