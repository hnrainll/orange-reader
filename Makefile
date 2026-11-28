.DEFAULT_GOAL := help

TEST_DATA_DIR ?= /private/tmp/verso-test
REF ?= main
APP_NAME := Verso.app
DIST_ZIP := dist/Verso-2.0.0-macos.zip

.PHONY: help run dev open self-test check test fmt fmt-check lint build package install update-foliate clean-dist clean

help: ## 显示可用命令
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z0-9_-]+:.*## / {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

run: ## 启动应用（可传 BOOK=/path/to/book.epub 直接打开）
	@if [ -n "$(BOOK)" ]; then \
		cargo run -- "$(BOOK)"; \
	else \
		cargo run; \
	fi

dev: ## 使用隔离数据目录启动应用（不触碰日常历史，可传 BOOK=...）
	@if [ -n "$(BOOK)" ]; then \
		VERSO_DATA_DIR="$(TEST_DATA_DIR)" cargo run -- "$(BOOK)"; \
	else \
		VERSO_DATA_DIR="$(TEST_DATA_DIR)" cargo run; \
	fi

open: ## 直接打开指定 EPUB（用法：make open BOOK=/path/to/book.epub）
	@test -n "$(BOOK)" || { echo "请指定书籍路径，例如：make open BOOK=/path/to/book.epub"; exit 1; }
	cargo run -- "$(BOOK)"

self-test: ## 使用隔离目录运行页内自测（用法：make self-test BOOK=/path/to/book.epub）
	@test -n "$(BOOK)" || { echo "请指定测试用 EPUB，例如：make self-test BOOK=/path/to/book.epub"; exit 1; }
	VERSO_DATA_DIR="$(TEST_DATA_DIR)" cargo run -- "$(BOOK)" --self-test

check: ## 编译检查与 Info.plist 校验
	plutil -lint packaging/macos/Info.plist
	cargo check

test: ## 运行单元测试
	cargo test

fmt: ## 格式化 Rust 代码
	cargo fmt

fmt-check: ## 检查 Rust 代码格式
	cargo fmt -- --check

lint: check ## 运行 clippy 与基础校验
	cargo clippy --all-targets

build: ## 构建 Release 可执行文件
	cargo build --release --locked

package: ## 打包 macOS .app 压缩包到 dist/
	scripts/package-macos.sh

install: package ## 打包并安装 Verso.app 到 /Applications
	@tmp_dir=$$(mktemp -d "$${TMPDIR:-/tmp}/verso-install.XXXXXX"); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	ditto -x -k "$(DIST_ZIP)" "$$tmp_dir"; \
	rm -rf "/Applications/$(APP_NAME)"; \
	cp -R "$$tmp_dir/$(APP_NAME)" "/Applications/$(APP_NAME)"; \
	echo "已安装到 /Applications/$(APP_NAME)"

update-foliate: ## 同步上游 foliate-js（可传 REF=<branch|commit>，默认 main）
	scripts/update-foliate-js.sh "$(REF)"

clean-dist: ## 清理 dist/ 与临时测试数据目录
	rm -rf dist "$(TEST_DATA_DIR)"

clean: clean-dist ## 清理所有构建与打包产物（含 target/）
	cargo clean
