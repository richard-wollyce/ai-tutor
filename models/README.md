# Modelos Soberanos Wollyce AI Tutor

Este diretório armazena os pesos quantizados (`.gguf`) para execução local soberana via `llama-server` (`llama.cpp`), sem dependência de internet ou APIs externas.

## Perfis Arquiteturais

### 1. Desktop Profile (Recomendado para macOS Apple Silicon / Linux / Windows)
- **Modelo:** DeepSeek R1 Distill Qwen 7B
- **Arquivo:** `DeepSeek-R1-Distill-Qwen-7B-Q4_K_M.gguf`
- **Contexto:** 8.192 tokens
- **VRAM / RAM:** ~4.5 GB (Metal GPU offload `-ngl 99`)
- **Papel:** Raciocínio socrático profundo, método dialético rigoroso, extração e síntese.

### 2. Mobile Profile (Android & iOS / Dispositivos com pouca RAM)
- **Modelo:** DeepSeek R1 Distill Qwen 1.5B
- **Arquivo:** `DeepSeek-R1-Distill-Qwen-1.5B-Q4_K_M.gguf`
- **Contexto:** 4.096 tokens
- **VRAM / RAM:** ~1.2 GB
- **Papel:** Execução local ultraleve em smartphones via Tauri Mobile.

## Instalação do Binário do Motor (llama-server)

No macOS com Homebrew:
```bash
brew install llama.cpp
```

No Linux:
```bash
# Compilar ou baixar release oficial do llama.cpp
git clone https://github.com/ggerganov/llama.cpp
cd llama.cpp && cmake -B build && cmake --build build --config Release -t llama-server
```

## Ciclo de Vida e Economia de Bateria

O Wollyce AI Tutor gerencia o `llama-server` automaticamente:
- **Liga / Desliga:** Controle direto na interface (barra superior).
- **Auto-Idle:** Desliga o `llama-server` automaticamente após **10 minutos** sem perguntas, liberando toda a RAM/GPU.
- **Encerramento Completo:** O botão "Sair" na interface finaliza de forma atômica tanto o servidor Rust quanto o `llama-server`, garantindo zero processos órfãos em segundo plano.
