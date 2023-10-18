-- Add migration script here
CREATE TABLE history (
  id      SERIAL        PRIMARY KEY,
  
  user_id BIGINT        NOT NULL,
  song_id SERIAL        NOT NULL,
  
  CONSTRAINT song_id_fk FOREIGN KEY(song_id) REFERENCES songs(id)
);

CREATE INDEX user_id_index ON history(user_id);