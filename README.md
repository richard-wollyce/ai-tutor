# Wollyce AI Tutor

> Um tutor socrático inteligente e 100% local que lê seus materiais e ensina o conteúdo de volta para você.

---

## 🎯 Por Que Este Projeto Existe?

A maioria das ferramentas de IA entrega respostas prontas. Isso cria a ilusão de aprendizado, mas não gera retenção real.

O **Wollyce AI Tutor** foi construído com um propósito claro: **ensinar e educar você a partir dos seus próprios materiais de estudo**.

Em vez de apenas responder, ele atua como um preceptor particular:
- Lê seus livros, anotações e resumos em PDF ou Markdown.
- Conduz diálogos socráticos e faz perguntas para testar sua retenção ativa.
- Explica tópicos complexos de forma intuitiva e progressiva.
- Roda de forma 100% offline, garantindo que suas notas nunca saiam da sua máquina.

---

## 💡 Como Funciona

1. **Você anexa seu material:** envie arquivos `.pdf`, `.txt` ou `.md`.
2. **A IA organiza localmente:** o sistema indexa o conteúdo na sua máquina sem enviar dados para a nuvem.
3. **O tutor orienta seus estudos:** faz perguntas socráticas, aplica a técnica Feynman e desafia você a demonstrar o que realmente aprendeu.

---

## 🛠️ Tecnologia Utilizada

- **IA Local Integrada:** motor preparado para rodar modelos abertos locais (DeepSeek R1 via `llama-server` ou Ollama) com desligamento automático após 10 minutos de ociosidade para poupar bateria e RAM.
- **Memória Determinística (Ulpia):** busca exata por palavras-chave com SQLite FTS5 (BM25), sem alucinações e sem bancos de vetores pesados.
- **Construído em Rust:** núcleo leve, rápido, seguro e sem dependências pesadas de nuvem.
- **Interface Minimalista:** visual moderno, digitação por voz, menu simplificado e inspeção do raciocínio dialético do modelo.

---

## 🚀 Como Executar

### 1. Pré-requisito
Certifique-se de ter o [Rust](https://rustup.rs/) instalado no seu computador.

### 2. Iniciar o Tutor
Na pasta do projeto, execute no terminal:

```bash
cargo run --release
```

A interface abrirá automaticamente no seu navegador em:
👉 **`http://127.0.0.1:4242`**

### 3. Encerrar
Para desligar o servidor e o motor local liberando 100% da memória, clique no botão **Sair** no topo da tela.

---

## 🔒 Privacidade Garantida

- Todo o material e histórico ficam salvos exclusivamente no seu dispositivo (em `data/`).
- Zero telemetria invasiva ou envio de anotações privadas para servidores de terceiros.
