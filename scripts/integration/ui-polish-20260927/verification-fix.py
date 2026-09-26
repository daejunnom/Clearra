import hashlib,json,os,pathlib,subprocess
BASE='521fee125b478a5a39a90a808d6c471c430c8494'
PARENT='6d1d5ca38f6019b62a640eb2976f5d38cc8ddd56'
BRANCH='codex/recovery-gray-fields-20260926'
def git(*args): return subprocess.check_output(['git',*args],text=True,timeout=120).strip()
def refs():
    for ref,sha in [('refs/heads/main',BASE),('refs/heads/'+BRANCH,PARENT)]:
        if git('ls-remote','origin',ref).split()!=[sha,ref]: raise RuntimeError('Reviewed ref moved')
refs()
git('checkout','--detach',PARENT)
if git('rev-parse','HEAD^{tree}')!='f9c42baba183dd2da362caf7fe7424cba22315b7': raise RuntimeError('Source tree mismatch')
changes=[
 ('crates/clearra-forward-search/src/boundary_recovery_auto.rs','875ba795a19b46b3444f8265d1d6c39a49ae1283',[
   ('assert_eq!(candidates(&query), vec![]);','assert!(candidates(&query).is_empty());')]),
 ('crates/clearra-cli-command/tests/boundary_recovery.rs','dc91ccf2d4662b82a3fe45771156eabffbce672c',[
   ('use clearra_app::{AppCommand, AppContext, AppStatus};','use clearra_app::{AppCommand, AppContext, AppErrorCode, AppStatus};'),
   ('    assert!(CliCommandParser::parse(base)\n        .map(|parsed| parsed.to_app_request().is_err())\n        .unwrap_or(true));',
    '    // Syntax compilation does not own the exact bag-role plan validation.\n    // Exercise its real execution boundary and require an input rejection,\n    // never a successful no-path result or an unavailable-runtime response.\n    let request = CliCommandParser::parse(base).unwrap().to_app_request().unwrap();\n    let response = AppContext::default().run(request);\n    assert_eq!(response.status(), AppStatus::ValidationFailed);\n    assert_eq!(response.error().unwrap().code(), AppErrorCode::InvalidInput);')])]
for name,before,replacements in changes:
    if git('hash-object',name)!=before: raise RuntimeError('Source blob mismatch: '+name)
    path=pathlib.Path(name);text=path.read_text(encoding='utf-8')
    for old,new in replacements:
        if text.count(old)!=1: raise RuntimeError('Exact edit not unique')
        text=text.replace(old,new)
    path.write_text(text,encoding='utf-8')
names=sorted(x[0] for x in changes)
git('add','--',*names)
if git('write-tree')!='970e5b430553e68cbb2326c3aada7eb040914479': raise RuntimeError('Exact repaired tree mismatch')
subprocess.run(['cargo','+1.98.1','fmt','--all'],check=True,timeout=120)
if git('diff','HEAD','--name-only').splitlines()!=names: raise RuntimeError('Unexpected formatter scope')
git('add','--',*names);git('diff','--cached','--check')
subprocess.run(['cargo','+1.98.1','fmt','--all','--check'],check=True,timeout=120)
tree=git('write-tree')
git('config','user.name','github-actions[bot]');git('config','user.email','41898282+github-actions[bot]@users.noreply.github.com')
git('commit','-m','test: verify automatic recovery through typed validation boundaries')
candidate=git('rev-parse','HEAD');refs();git('push','origin',candidate+':refs/heads/'+BRANCH)
if git('ls-remote','origin','refs/heads/'+BRANCH).split()[0]!=candidate: raise RuntimeError('Candidate readback mismatch')
out=pathlib.Path(os.environ['RUNNER_TEMP'])/'ui-polish-source';out.mkdir()
(out/'complete.patch').write_text(git('diff','--binary','246e1c5ee9a743ab9a24ba731af55ccc6b805ddd',candidate)+'\n',encoding='utf-8')
receipt={'source_commit':candidate,'parent':PARENT,'base':BASE,'tree':tree,'main_written':False,
         'files':git('diff','--name-only','246e1c5ee9a743ab9a24ba731af55ccc6b805ddd',candidate).splitlines()}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt),flush=True)
subprocess.run(['gh','workflow','run','management-policy.yml','--ref',BRANCH],check=True,timeout=120)
with open(os.environ['GITHUB_OUTPUT'],'a') as f:f.write('candidate='+candidate+'\ntree='+tree+'\n')
