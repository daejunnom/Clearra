import { spawn } from 'node:child_process';

// Every fixture has its own short end point. A broken supervisor must not
// turn a lifecycle regression test into an unbounded computation.
switch (process.argv[2]) {
  case 'normal':
    setTimeout(() => console.log('fixture-complete'), 150);
    break;
  case 'cancel':
    console.log('fixture-ready');
    setTimeout(() => console.log('unexpected-late-success'), 2_500);
    break;
  case 'descendant': {
    const child = spawn(process.execPath, [process.argv[1], 'descendant-child'], {
      stdio: 'inherit',
    });
    child.unref();
    console.log('fixture-descendant-pid=' + child.pid);
    setTimeout(() => process.exit(0), 100);
    break;
  }
  case 'descendant-child':
    setTimeout(() => console.log('unexpected-descendant-success'), 2_500);
    break;
  default:
    throw new Error('Unknown owner-lifetime fixture');
}
