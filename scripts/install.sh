#!/usr/bin/env bash
set -euo pipefail

# Script de instalação do FastFile para Linux / macOS (§3.3 e #23).
# Instala tanto o executável 'fastfile' quanto o alias 'ff'.

INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
USER_MODE=false

if [ ! -w "$INSTALL_DIR" ]; then
    INSTALL_DIR="$HOME/.local/bin"
    USER_MODE=true
fi

mkdir -p "$INSTALL_DIR"

echo "==> Instalando FastFile em $INSTALL_DIR..."

if command -v cargo >/dev/null 2>&1; then
    echo "==> Compilando via cargo..."
    cargo build --release --workspace
    cp target/release/ff "$INSTALL_DIR/ff"
    cp target/release/fastfile "$INSTALL_DIR/fastfile"
    chmod +x "$INSTALL_DIR/ff" "$INSTALL_DIR/fastfile"
else
    echo "ERRO: cargo não encontrado. Instale Rust (https://rustup.rs) ou baixe o binário de release." >&2
    exit 1
fi

echo "==> FastFile instalado com sucesso:"
echo "    - $INSTALL_DIR/ff"
echo "    - $INSTALL_DIR/fastfile"

if [ "$USER_MODE" = true ] && [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    echo "AVISO: Adicione $HOME/.local/bin ao seu PATH se necessário:"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
fi
