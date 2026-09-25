"""Prepare the reviewed candidate only. Never update main, tags or deployments."""
import json, os, pathlib, subprocess, tempfile

MAIN = '3e06bce344da89212b002d0cba96063703088074'
CANDIDATE = '77880df9c2b0e919cacf5b10145cfab9e5f2b1e8'
TARGET = 'codex/candidate-pinned-boundary-release-20260925'
ROWS = '''pc-all-worker-scheduling-20260912 c73a214a7f0e3bb6faca80c3f294237229310e37
v0.8.1-parallelism-range-20260912 19fcb4f438c04162217115cbbbd5a22adaaa85ed
v0.8.1-parallel-abba-20260920 65f06c16c296877ad7f98e8313e95e26ebb68bd3
v0.8.1-worker-tail-audit-20260912 2206308c08e3dea8b5416267853e13c38657e63a
japanese-i18n-prep a83a38b88527b3aae31c14834b86e93f9a23d512
japanese-translation-20260910 eada4984deae464ab1e3d0e53560c463ad329973
next-gui-render-watchdog 332c92e81b04592987d0425746ca310b28f8f761
pc-replay-dp-20260906 8a70c05bb9db60b49bf820cea3896450ed3bc1a8
v0.8.1-copy-contract-20260912 02417bf22072058d42ee442224c0c77fa4cc9a36
v0.8.1-completion-audit-20260912 b99a086d95b8b29fcc787d916273bb724766305b
v0.8.1-product-gap-audit-20260912 c3099ac2a598e38a30e0cf92b135aa31bda6ec7a
v0.8.1-product-parity-round2-20260912 b0993a0871cf34775e4c7bf93397a9e1cfafe7e4
v081-product-parity-closure-20260912 6246ee9f553880efdad39705a96b3970e4a13ed0
actions-tail-parallelism f7ef020d5cecf74138292d32d9da04b4f03aa3c5
ci-independent-failure-collection-next 4ef44848048ae46fc0d9cf22131633093952383c
fix-pages-bootstrap 0c4f96e9149e6a774669f77d549e298f54fdac8e
fix-release-publish fcaa26c252143d80d5e2be60ea4c7d639f2a31a9
gate-independent-failure-collection 300c468f9535cf1957e5d9789cce29138f80833f
next-fast-deploy 94ed0e79e21670f9d893c340e429abd2fd639f73
pages-legacy-contract-fix 15764945f904a155d8cfa115b45edb226f01974d
release-build-architecture-ab 03456ed5b2a3554cfe5d133c8629c3a2eff929d3
release-pages 2520886b33bfedaa552c23070fca37ef8060b173
release-rust-shard-contention e1e0b13541ab17e7788c99f00d026eb989938c54
release-rust-shard-contention-ci aea538f61296c918950f9fd303362b47e14518cf
release-rust-shard-contention-ci-v2 d7d4e6b2bb559716363533f2d0e9742419094be3
release-rust-shard-contention-ci-v3 89beb449d58cf09c570fc9af53ee1e3187709350
v0.8.1-strict-clippy-20260912 e3aabe5cd166e85adde7e28f1d6bbbcdfbd02380'''
EXPECTED = {
 'core-c/CMakeLists.txt':'9ee0383e45b118889f4c30d71fb1cacf9202c205',
 'core-c/cmake/library_target.cmake':'7fdc8b83dff74eda3e8f4ee3080d89bdcbde38e5',
 'crates/clearra-core-executor/src/backend/wasm_cpu_search_backend.rs':'2f42d9fa175f82478d6fc6a480d4290a6391f5e2',
 'crates/clearra-forward-search/src/boundary_recovery.rs':'adc49c677b75fbbbc3ae09e60092b546adac4130',
 'scripts/architecture/validate_no_product_debt.ps1':'d9938ae999cf29fd0a7340f87fd73e7ce310dd53',
 'scripts/lib/rust-exact-tests.ps1':'919a36fdc20632da44a8099deaa0a7d5d998d779',
 'scripts/test_release_acceptance_shards.ps1':'1ee522fe64b404158278d8f49f8ce8ac09c77697',
 'scripts/tools/clearra-frontend-paths.mjs':'bf4e19cfd89b9b06068e34f849df3f97f32374fa',
 'core-c/cmake/reproducible_msvc_archive.cmake':'97db93f49fe8b3c6632b18dbf1895fa4a19f06a9',
 'crates/clearra-forward-search/src/boundary_recovery_tests.rs':'200efae499e4dc9752f611594cf09656bc97df06',
 'docs/research/minimum-parallel-overhead-and-algorithm-plan-2026-09-08.md':'725a6c24109b882bc73aa564ae37159b94d9550c',
 'docs/research/release-build-architecture-ab-2026-09-09.md':'19fcf2d74ea19344116c3e06ec382c3c8e34c62e',
 'docs/research/release-rust-shard-contention-ab-2026-09-09.md':'786eae124cc0a0b5fb61c5775afd567a20d16930',
 'scripts/tools/clearra-build-filesystem-path.mjs':'2f77e617d8fbbb901f8405722c8d57fed9eebcfb',
 'scripts/tools/clearra-build-filesystem-path.test.mjs':'698b35a76d95a574604edc55a4d1d50917be628e',
}

def run(*args, cwd=None, data=None, check=True):
    p=subprocess.run(list(args),cwd=cwd,input=data,capture_output=True)
    if check and p.returncode: raise RuntimeError(p.stderr.decode('utf8','replace'))
    return p

def main():
    original=pathlib.Path.cwd()
    def git(*args, **kwargs): return run('git',*args,cwd=original,**kwargs)
    actual=git('ls-remote','origin','refs/heads/main').stdout.decode().split()[0]
    if actual!=MAIN: raise RuntimeError('main moved; reconcile again before writing')
    target=git('ls-remote','origin','refs/heads/'+TARGET).stdout.decode().split()[0]
    if target!=CANDIDATE: raise RuntimeError('candidate moved; never overwrite concurrent work')
    git('merge-base','--is-ancestor',MAIN,CANDIDATE)
    parents=[line.split() for line in ROWS.splitlines()]
    for _,sha in parents: git('cat-file','-e',sha+'^{commit}')
    output=pathlib.Path(os.environ['RUNNER_TEMP'])/'reviewed-integration'
    output.mkdir(exist_ok=True)
    work=output/'source'
    git('worktree','add','--detach',str(work),CANDIDATE)
    def show(ref,path):return git('show',ref+':'+path).stdout.decode('utf8')
    def write(path,text):
        p=work/path;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(text,encoding='utf8',newline='\n')
    def replace(path,old,new):
        p=work/path;text=p.read_text(encoding='utf8')
        if text.count(old)!=1:raise RuntimeError('reviewed edit context differs: '+path)
        write(path,text.replace(old,new,1))
    # Use only clean three-way merges for the six reviewed build/test files;
    # compare their complete resulting blobs to the locally reviewed tree below.
    head='03456ed5b2a3554cfe5d133c8629c3a2eff929d3'
    base=git('merge-base',MAIN,head).stdout.decode().strip()
    merge_paths=['core-c/CMakeLists.txt','core-c/cmake/library_target.cmake',
      'crates/clearra-core-executor/src/backend/wasm_cpu_search_backend.rs',
      'scripts/lib/rust-exact-tests.ps1','scripts/test_release_acceptance_shards.ps1']
    with tempfile.TemporaryDirectory(dir=output) as tmp:
        for path in merge_paths:
            names=[pathlib.Path(tmp)/x for x in ['ours','base','incoming']]
            for p,ref in zip(names,[CANDIDATE,base,head]):p.write_text(show(ref,path),encoding='utf8')
            merged=run('git','merge-file','-p',*[str(p) for p in names],check=False)
            if merged.returncode:raise RuntimeError('unexpected merge conflict: '+path)
            write(path,merged.stdout.decode('utf8'))
    path='core-c/cmake/reproducible_msvc_archive.cmake'
    write(path,show(head,path).replace('keeps unchanged C inputs byte-identical across hosted runners.',
      'keeps identical inputs byte-identical at the same path/toolchain.'))
    path='scripts/architecture/validate_no_product_debt.ps1'
    replace(path,". (Join-Path $PSScriptRoot '../lib/clearra-local-diagnostics-policy.ps1')",
      ". (Join-Path $PSScriptRoot '../lib/clearra-local-diagnostics-policy.ps1')\n. (Join-Path $PSScriptRoot '../lib/clearra-path-helpers.ps1')")
    replace(path,"""    foreach ($forbiddenRoot in @('target', 'build')) {
        $forbiddenPath = Join-Path $Root $forbiddenRoot
        if (Test-Path -LiteralPath $forbiddenPath) {
            Add-ArchitectureError "NoProductDebt repository-local artifact directory exists: $forbiddenRoot"
        }
    }
    try { Assert-ClearraLocalToolDirectoryPolicy $Root.Path }""","""    # One authority owns declared output roots, Git exclusion and link safety.
    # A declared ignored build/ root is allowed; target/, undeclared or tracked
    # output and junction/symlink escapes remain release-blocking.
    try { Assert-ClearraRepositoryArtifactPolicy $Root.Path }""")
    path='crates/clearra-forward-search/src/boundary_recovery.rs'
    source=(work/path).read_text();marker='#[cfg(test)]\nmod tests {'
    if source.count(marker)!=1:raise RuntimeError('Boundary test module changed')
    production,tests=source.split(marker)
    if not tests.rstrip().endswith('}'):raise RuntimeError('invalid test module boundary')
    inner=tests.rstrip()[:-1].strip('\n')
    inner='\n'.join(line[4:] if line.startswith('    ') else line for line in inner.splitlines())+'\n'
    write('crates/clearra-forward-search/src/boundary_recovery_tests.rs',inner)
    write(path,production+'#[cfg(test)]\n#[path = "boundary_recovery_tests.rs"]\nmod tests;\n')
    for ref,path in [
      ('a83a38b88527b3aae31c14834b86e93f9a23d512','docs/research/minimum-parallel-overhead-and-algorithm-plan-2026-09-08.md'),
      (head,'docs/research/release-build-architecture-ab-2026-09-09.md'),
      (head,'docs/research/release-rust-shard-contention-ab-2026-09-09.md')]:write(path,show(ref,path))
    # New, reviewed source lives on the control branch; copying is verified by
    # full blob identity and never introduces the control workflow into product.
    for path in ['scripts/tools/clearra-build-filesystem-path.mjs','scripts/tools/clearra-build-filesystem-path.test.mjs']:
        write(path,(original/path).read_text(encoding='utf8'))
    path='scripts/tools/clearra-frontend-paths.mjs'
    replace(path,"import { execFileSync } from 'node:child_process';",
      "import { execFileSync } from 'node:child_process';\nimport { resolveBuildFilesystemPath } from './clearra-build-filesystem-path.mjs';")
    replace(path,'const transactionRoot = hasOwner','const selectedTransactionRoot = hasOwner')
    replace(path,"  const frontendRoot = resolve(transactionRoot, 'frontend', app);",
      "  // The owner identity is case-insensitive, but Vite manifest keys are not.\n  const transactionRoot = resolveBuildFilesystemPath(selectedTransactionRoot);\n  const frontendRoot = resolveBuildFilesystemPath(resolve(transactionRoot, 'frontend', app));")
    for path,expected in EXPECTED.items():
        actual=run('git','hash-object','--',path,cwd=work).stdout.decode().strip()
        if actual!=expected:raise RuntimeError('reviewed output blob mismatch: '+path+' '+actual)
    records=[]
    for name,sha in parents:
        cherry=git('cherry','-v',CANDIDATE,sha).stdout.decode().splitlines()
        disposition='already-patch-equivalent-preserve-newer-candidate'
        if any(x.startswith('+') for x in cherry):
            if name.startswith('release-rust-shard') or name=='release-build-architecture-ab':
                disposition='final-isolated-harness-and-Brepro-integrated; rejected-cache-trials-research-only; newer-CLI-producer-preserved'
            elif name=='japanese-i18n-prep':disposition='research-note-retained; obsolete-plan-replacement-not-restored'
            elif name=='pc-replay-dp-20260906':disposition='newer-PieceDecision-language-canonical-profile-digest-and-drained-worker-replacement-preserved'
            elif name=='next-fast-deploy':disposition='superseded-by-current-FastFix-component-ledger-and-recovery; historical-design-retained'
            else:disposition='independent-failure-collection-preserved-in-newer-candidate-topology'
        records.append({'branch':'codex/'+name,'head':sha,'resolution':disposition,
          'patch_equivalent_commits':sum(x.startswith('-') for x in cherry),
          'non_equivalent_commits':sum(x.startswith('+') for x in cherry)})
    write('docs/integration/approved-branches-2026-09-25.json',json.dumps(records,indent=2)+'\n')
    write('docs/integration/reviewed-27-branches-2026-09-25.md', '''# Reviewed 27-branch integration — 2026-09-25

This is a semantic reconciliation on candidate `77880df9c2b0e919cacf5b10145cfab9e5f2b1e8`,
not blanket selection of old branch files. The exact branch heads and decisions
are in `approved-branches-2026-09-25.json`. All reviewed histories are retained as
parents; patch-equivalent or subsequently corrected behavior keeps the newer
candidate implementation. A history receipt is not a test pass.

PC root scheduling, adaptive Forward batches, control-only minimum managers,
complete result draining, larger-worker replacement and canonical PieceDecision
replay semantics are preserved. In particular, verifier replacement is forbidden
until geometry completion and output-lease draining; completed work is not lost
when a warm minimum worker becomes a geometry verifier.

M4/checker pruning is retained. Its theoretical basis is LLY's *Four-colour
Parity Theory: Parities and 4-remainders*, supplied for review (2024-10-20 update).
The implementation uses actual catalog rows and normalized target-frame cells;
its additive domain is necessary, not sufficient. Out-of-domain states fail open.
No current solver algorithm is replaced with a rejected A/B experiment.

The final Rust harness scheduler compiles a complete inventory once and isolates
process-global resource owners while allowing bounded independent processes.
No tests are removed. The declared ignored build root is checked by the existing
root authority rather than a contradictory blanket blacklist. Boundary Recovery
tests are separated from the engine without changing the implementation.

Windows compiler paths preserve native filesystem spelling; case-folded strings
remain ownership identities only. A pinned Windows SvelteKit/Vite fixture
reproduced the manifest failure for lower-case aliases and passed with physical
spelling. Path/link/manifest validation is not bypassed.

Current Fast Fix/component-ledger finalization is the single selective release
authority. The old Fast Correction dual-source route would conflict with current
Discord recovery/checkpoint ownership and is not reintroduced. Its source history
and historical design are retained. Rejected cache A/B implementations likewise
do not enter default execution or CI triggers.

Main may only fast-forward to the exact candidate after the nonpublishing gate
passes. Canonical Product Release and Pages publication are separate operations;
the integration workflow does not invoke them, change main or create tags.
''')
    old=show('94ed0e79e21670f9d893c340e429abd2fd639f73','docs/fast-correction-deploy.md')
    write('docs/history/fast-correction-deploy-2026-09-25.md',
      '# Superseded Fast Correction design\n\nHistorical record only; current Fast Fix/component-ledger workflows are the execution authority. Do not use the following as current deployment instructions.\n\n---\n\n'+old)
    run('git','diff','--check',cwd=work)
    run('git','add','--',*EXPECTED.keys(),'docs/integration','docs/history/fast-correction-deploy-2026-09-25.md',cwd=work)
    tree=run('git','write-tree',cwd=work).stdout.decode().strip()
    args=['git','commit-tree',tree,'-p',CANDIDATE]
    for _,sha in parents:args+=['-p',sha]
    args+=['-m','merge: reconcile 27 reviewed branches, preserve runtime fixes and repair parallel acceptance']
    commit=run(*args,cwd=work).stdout.decode().strip()
    run('git','merge-base','--is-ancestor',MAIN,commit,cwd=work)
    if git('ls-remote','origin','refs/heads/main').stdout.decode().split()[0]!=MAIN:raise RuntimeError('main moved during preparation')
    if git('ls-remote','origin','refs/heads/'+TARGET).stdout.decode().split()[0]!=CANDIDATE:raise RuntimeError('candidate moved during preparation')
    run('git','push','origin',commit+':refs/heads/'+TARGET,cwd=work)
    receipt={'main':MAIN,'base_candidate':CANDIDATE,'candidate':commit,'tree':tree,'branches':records,
      'expected_changed_blobs':EXPECTED,'main_updated':False,'publication':'not-requested-by-this-workflow'}
    (output/'receipt.json').write_text(json.dumps(receipt,indent=2))
    with open(os.environ['GITHUB_OUTPUT'],'a') as f:f.write('candidate='+commit+'\n')
    print(json.dumps({'candidate':commit,'reviewed_branches':len(records),'main_updated':False}))

if __name__=='__main__': main()
