#!/bin/sh
touch /data/sprite.db
# no-op when db already exists
/app/sqlx db create

/app/sprite