-- Add migration script here
CREATE TABLE likes (
    liked_by BIGINT NOT NULL,
    song_id  BIGINT NOT NULL,

    sent_by   BIGINT    NOT NULL,
    like_date TIMESTAMP NOT NULL,
    liked     BOOL      NOT NULL,

    PRIMARY KEY (liked_by, song_id),
    CONSTRAINT likes_song_id FOREIGN KEY(song_id) REFERENCES songs(id),
);
