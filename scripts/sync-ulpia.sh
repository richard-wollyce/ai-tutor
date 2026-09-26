#!/usr/bin/env bash
# Sincroniza os crates locais do AI Tutor (crates/kb e crates/model-call)
# diretamente a partir do repositório irmão do Ulpia (../ulpia/tools/kb e ../ulpia/tools/model-call).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AI_TUTOR_DIR="$(dirname "$SCRIPT_DIR")"
ULPIA_DIR="${1:-$AI_TUTOR_DIR/../ulpia}"

if [ ! -d "$ULPIA_DIR/tools/kb" ]; then
    echo "Erro: Repositório Ulpia não encontrado em: $ULPIA_DIR"
    echo "Uso: ./scripts/sync-ulpia.sh [caminho_para_ulpia]"
    exit 1
fi

echo "==> Sincronizando crates a partir de $ULPIA_DIR..."

# Sincroniza crates/kb
echo "• Sincronizando crates/kb..."
rsync -av --delete \
    --exclude "target" \
    --exclude "bin" \
    --exclude ".DS_Store" \
    "$ULPIA_DIR/tools/kb/" "$AI_TUTOR_DIR/crates/kb/"

# Sincroniza crates/model-call
echo "• Sincronizando crates/model-call..."
rsync -av --delete \
    --exclude "target" \
    --exclude ".DS_Store" \
    "$ULPIA_DIR/tools/model-call/" "$AI_TUTOR_DIR/crates/model-call/"

# Normaliza finais de linha para LF
echo "• Normalizando quebras de linha para LF..."
find "$AI_TUTOR_DIR/crates" -type f \( -name "*.rs" -o -name "*.toml" -o -name "*.md" \) -exec perl -pi -e 's/\r\n/\n/g' {} +

echo "==> Sincronização concluída com sucesso!"
cargo check --manifest-path "$AI_TUTOR_DIR/Cargo.toml"
