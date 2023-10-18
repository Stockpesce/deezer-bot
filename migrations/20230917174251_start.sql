-- Add migration script here
CREATE TABLE songs (
  id        SERIAL        PRIMARY KEY,
  deezer_id BIGINT        UNIQUE NOT NULL,
  file_id   VARCHAR(100)  NOT NULL
);

CREATE INDEX deezer_id_index ON songs(deezer_id);