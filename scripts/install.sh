#!/usr/bin/env bash
# FastFile — Script de Instalação para macOS (Apple Silicon / Intel) e Linux
set -euo pipefail

echo "=== FastFile — Instalador para macOS e Linux ==="

OS_TYPE="$(uname -s)"
ARCH_TYPE="$(uname -m)"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

case "$OS_TYPE" in
    Darwin)
        echo "Sistema detectado: macOS ($ARCH_TYPE)"
        ;;
    Linux)
        echo "Sistema detectado: Linux ($ARCH_TYPE)"
        ;;
    *)
        echo "Aviso: Sistema operacional '$OS_TYPE' não suportado oficialmente por este script."
        ;;
esac

# 1. Compila os binários em modo release se executado a partir do código-fonte
if [ -f "Cargo.toml" ]; then
    echo "[1/3] Compilando binários em release..."
    cargo build --release --bin fastfile --bin ff --bin ff-ui --bin ff-indexer
    SRC_DIR="target/release"
elif [ -d "target/release" ]; then
    SRC_DIR="target/release"
else
    SRC_DIR="."
fi

# 2. Copia os binários para o diretório de destino
echo "[2/3] Instalando binários em $INSTALL_DIR..."
if [ ! -w "$INSTALL_DIR" ]; then
    SUDO="sudo"
    echo "Permissões de administrador necessárias para instalar em $INSTALL_DIR."
else
    SUDO=""
fi

$SUDO cp -f "$SRC_DIR/fastfile" "$INSTALL_DIR/fastfile"
$SUDO cp -f "$SRC_DIR/ff" "$INSTALL_DIR/ff"
if [ -f "$SRC_DIR/ff-ui" ]; then
    $SUDO cp -f "$SRC_DIR/ff-ui" "$INSTALL_DIR/ff-ui"
fi
if [ -f "$SRC_DIR/ff-indexer" ]; then
    $SUDO cp -f "$SRC_DIR/ff-indexer" "$INSTALL_DIR/ff-indexer"
fi

$SUDO chmod +x "$INSTALL_DIR/fastfile" "$INSTALL_DIR/ff"
if [ -f "$INSTALL_DIR/ff-ui" ]; then
    $SUDO chmod +x "$INSTALL_DIR/ff-ui"
fi
if [ -f "$INSTALL_DIR/ff-indexer" ]; then
    $SUDO chmod +x "$INSTALL_DIR/ff-indexer"
fi

# 3. Verificação
echo "[3/3] Validando instalação..."
"$INSTALL_DIR/fastfile" --help > /dev/null
"$INSTALL_DIR/ff" --help > /dev/null

echo "=== Instalação concluída com sucesso! ==="
echo "Comandos disponíveis:"
echo "  fastfile --help       (CLI unificada / agentes)"
echo "  ff --help             (CLI rápida de busca e utilitários)"
if [ -f "$INSTALL_DIR/ff-ui" ]; then
    echo "  ff-ui                 (Interface gráfica nativa egui)"
fi
