#!/bin/bash
set -e

echo "Setting up worktree environment..."

# 共有DBを使用する設定
cat > backend/.env <<EOF
DATABASE_URL=postgresql://root@localhost:26257/y_junction?sslmode=disable
TEST_DATABASE_URL=postgresql://root@localhost:26257/y_junction_test?sslmode=disable
EOF

echo "✅ Setup complete!"
echo ""
echo "Using shared database: y_junction"
echo "- No import needed (uses main worktree's data)"
echo "- Run: cd backend && cargo test"
echo ""
echo "If you need a separate DB for schema changes, create one manually:"
echo "  docker exec y-junctions-cockroachdb ./cockroach sql --insecure -e 'CREATE DATABASE my_feature_db;'"
echo "  echo 'DATABASE_URL=postgresql://root@localhost:26257/my_feature_db?sslmode=disable' > backend/.env"
