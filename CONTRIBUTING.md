# 开发与贡献

## Clippy pedantic 维护约定

工作区在 `rust/Cargo.toml` 中保持 `clippy::pedantic = "warn"`，严格验收使用 `-D warnings`。目标是让诊断持续有效，而不是把豁免数量降到零。

### 豁免原则

- 禁止新增 `clippy::pedantic`、`clippy::all`、`warnings` 等组豁免，不通过提高 lint 阈值掩盖问题。
- 不在 crate、文件或无关模块上放置广域豁免。必要例外必须缩小到实际函数、结构或表达式，并附具体 `reason`。
- `expect` 也是豁免；`allow -> expect` 不算问题消除。保留 `expect` 时必须确保其在实际编译项上会被触发，并检查 `unfulfilled_lint_expectations`。
- 理由应说明值域、低位/补码、ABI/对齐、公开接口所有权或资源生命周期，不能只写“兼容 C”或“保持行为”。
- 不添加无意义的 `clone`、`drop`、`Copy` 或包装层来压制 lint。

### 按模块顺手复核

修改模块时只审查本次触及的函数及邻近豁免，不重新开展全仓库清理：

1. 运行对应 crate 的严格 Clippy。
2. 对私有、无 ABI/协议约束且有测试覆盖的候选项，优先做真实等价修复，例如私有 helper 借用化、局部声明前移和有证明的受检转换。
3. 公开 Rust API、C/Win32 ABI、JSON 与操作协议、有效输入输出、取消与错误优先级、回滚和提交顺序保持不变。
4. 协议低位、回绕、非对齐读取、公开 owned 参数和数值算法只在证明安全时修改；否则保留最小例外。
5. 畸形输入处理、协议范围和资源策略的变化必须作为独立行为修复评审，不混入 lint 清理。
6. 运行相关行为测试，并复核修改后的展开诊断。局部属性数量可能因收窄而增加，不以数量下降作为唯一指标。

### 可复验命令

从仓库根目录、初始化现有 MSVC 环境后运行：

```powershell
cargo +1.99.0 fmt --manifest-path rust/Cargo.toml --all -- --check
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --workspace --all-targets --all-features --locked --offline -- -D warnings"
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --workspace --all-targets --locked --offline -- -D warnings"
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --workspace --all-targets --no-default-features --locked --offline -- -D warnings"
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc -p dvda-core --all-targets --no-default-features --locked --offline -- -D warnings"
```

审计被豁免的诊断时，分别收集 feature 配置：

```powershell
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --workspace --all-targets --all-features --locked --offline --message-format=json -- --force-warn clippy::pedantic"
```

展开审计退出成功不代表零诊断。比较时按首个 primary span 的规则、文件和行号去重，保持相同工具链与 feature，并同时检查 `unfulfilled_lint_expectations`。workspace 的 `no-default-features` 不会覆盖依赖重新启用的 Core 默认 feature，因此必须保留上面的 Core 独立检查。

构建报告写入被忽略的 `build/clippy-audit/`，不作为发布物。缺外部 oracle、FFmpeg、媒体 fixture、其他卷或 ARM64 原生库的测试必须标记 NOT RUN，不能计为通过。
