#!/bin/bash
set -e

echo "Setting up worktree environment..."

# 共有DBを使用する設定
cat > backend/.env <<EOF
DATABASE_URL=postgres://y_junction:y_junction@localhost:5432/y_junction
TEST_DATABASE_URL=postgres://y_junction:y_junction@localhost:5432/y_junction_test
EOF

echo "✅ Setup complete!"
echo ""
echo "Using shared database: y_junction"
echo "- No import needed (uses main worktree's data)"
echo "- Run: cd backend && cargo test"
echo ""
echo "If you need a separate DB for schema changes, create one manually:"
echo "  docker exec y-junctions-db psql -U y_junction -c 'CREATE DATABASE my_feature_db TEMPLATE y_junction;'"
echo "  echo 'DATABASE_URL=postgres://y_junction:y_junction@localhost:5432/my_feature_db' > backend/.env"
