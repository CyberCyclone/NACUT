# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

## Claude Code specifics

- To check a UI change, render it with `--render-to` (see AGENTS.md) and open the PNGs with the Read tool rather than
  asking the user for screenshots.
- This app reads Claude Code's own data: transcripts in `~/.claude/projects/**/*.jsonl` and the Claude Code sign-in.
  Running the real app or `nacut summary` on this machine therefore reports this session's usage too.
