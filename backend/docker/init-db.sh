#!/bin/bash
set -e

# テスト用データベースを作成
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    CREATE DATABASE y_junction_test;
    GRANT ALL PRIVILEGES ON DATABASE y_junction_test TO y_junction;
EOSQL

echo "Test database 'y_junction_test' created successfully"
