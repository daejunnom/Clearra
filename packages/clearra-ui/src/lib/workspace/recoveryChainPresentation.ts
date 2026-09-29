/** Shared-frame chain transport and physical replay checks. No heuristic
 * reachability or independent-stage probability multiplication is performed. */
import type { RecoveryBuildPayload } from './recoveryBuildPayloadTypes';
import { compactRecoveryBoard, countRecoveryCells } from './recoveryBuildModel';
import { decimal, hex, piece, number, flag, requireEvidence as require, validateRecoverySummary } from './recoveryBuildValidation';
function mirror(mask:bigint,height:number):bigint {
  let result=0n;
  for(let y=0;y<height;y++)for(let x=0;x<10;x++)if(mask&(1n<<BigInt(y*10+x)))result|=1n<<BigInt(y*10+9-x);
  return result;
}
function requireOrientations(start:bigint,original:bigint[],actual:bigint[],height:number):void {
  let parities=[false],prefix=start;
  for(let i=0;i<actual.length;i++){
    const base=compactRecoveryBoard(prefix,height), symmetric=mirror(base,height)===base;
    const next=new Set<boolean>();
    for(const p of parities)for(const toggle of [false,true]){
      if(toggle && !symmetric)continue;
      const parity=p!==toggle;
      if(actual[i]===(parity?mirror(original[i],height):original[i])) next.add(parity);
    }
    require(next.size>0 && !(prefix&actual[i]));prefix|=actual[i];parities=[...next];
  }
}
export function validateRecoveryChainPayload(p:RecoveryBuildPayload):void {
  require(p && /^[0-9a-f]{64}$/u.test(p.input_identity) && number(p.height) && p.height>=1 && p.height<=24);
  const bound=1n<<BigInt(p.height*10), fits=(x:unknown)=>hex(x)&&BigInt(x)<bound;
  require(fits(p.start_board_mask) && Array.isArray(p.stage_targets) && p.stage_targets.length>=2 && p.stage_targets.length<=60);
  const targets=p.stage_targets;
  require(Array.isArray(p.stage_supplies) && p.stage_supplies.length===targets.length && p.stage_supplies.every(s=>typeof s==='string' && s.trim()));
  require(targets.every(fits) && targets[0]===p.middle_target_mask && targets[targets.length-1]===p.result_target_mask);
  require(p.stage_supplies[0]===p.first_supply && p.stage_supplies[p.stage_supplies.length-1]===p.second_supply);
  const original=targets.map(BigInt),n=original.map(mask=>countRecoveryCells(mask)/4),start=BigInt(p.start_board_mask);
  require(n.every(x=>Number.isInteger(x) && x>0));
  let all=start;for(const t of original){require(!(all&t));all|=t;}
  validateRecoverySummary(p);
  const maximum=n.slice(0,-1).map((_,i)=>Math.min(n.slice(i+1).reduce((a,b)=>a+b,0),p.early_limit===null?Infinity:Number(p.early_limit)));
  for(const e of [...p.examples,...(p.solutions ?? []).map(s=>s.example)]){
    require(['normal','recovery'].includes(e.status) && decimal(e.first_pattern) && decimal(e.second_pattern));
    require(Array.isArray(e.stage_target_masks) && e.stage_target_masks.length===n.length && e.stage_target_masks.every(fits));
    const actual=e.stage_target_masks.map(BigInt);
    requireOrientations(start,original,actual,p.height);
    require(e.middle_target_mask===e.stage_target_masks[0] && e.result_target_mask===e.stage_target_masks.at(-1));
    require(actual.every((m,i)=>countRecoveryCells(m)===n[i]*4));
    require(Array.isArray(e.stage_queues) && e.stage_queues.length===n.length && e.stage_queues.every(s=>typeof s==='string' && /^[IJLOSTZ]+$/u.test(s)));
    require(e.first_queue===e.stage_queues[0] && e.second_queue===e.stage_queues[1]);
    require(Array.isArray(e.stage_patterns) && e.stage_patterns.length===n.length && e.stage_patterns.every(decimal));
    require(e.first_pattern===e.stage_patterns[0] && e.second_pattern===e.stage_patterns[1]);
    require(Array.isArray(e.steps) && e.steps.length===n.reduce((a,b)=>a+b,0));
    require(Array.isArray(e.placement_stages) && e.placement_stages.length===e.steps.length && e.placement_stages.every(i=>number(i)&&i>=0&&i<n.length));
    require(Array.isArray(e.early_by_boundary) && e.early_by_boundary.length===n.length-1 && e.early_by_boundary.every((v,i)=>number(v)&&v>=0&&v<=maximum[i]));
    require(decimal(e.actual_early) && e.actual_early===String(Math.max(...e.early_by_boundary)) && e.effective_max_early===String(Math.max(...maximum)));
    require(Array.isArray(e.exchange_balance) && e.exchange_balance.length===7 && e.exchange_balance.every(number));
    const queues=e.stage_queues,combined=queues.join(''),ends:number[]=[];
    let end=0;for(const q of queues){end+=q.length;ends.push(end);}
    let active:number|null=0,held:number|null=null,cursor=1,board=compactRecoveryBoard(start,p.height),b2b=p.initial_b2b;
    const used=Array<bigint>(n.length).fill(0n),usedOrigins=Array<number>(n.length).fill(0),early=Array<number>(n.length-1).fill(0);
    const supplyCounts=n.map(()=>Array<number>(7).fill(0)),targetCounts=n.map(()=>Array<number>(7).fill(0)),usedSources=new Set<number>(),deleted=new Set<number>();
    for(let y=0;y<p.height;y++)if(((start>>BigInt(y*10))&1023n)===1023n)deleted.add(y);
    for(let i=0;i<e.steps.length;i++){
      const step=e.steps[i],stage=e.placement_stages[i];
      require(decimal(step.source_index)&&piece(step.piece)&&[step.result_target,step.recognized_spin,step.b2b_active,step.middle_complete].every(flag));
      require(number(step.rotation)&&step.rotation>=0&&step.rotation<4&&number(step.x)&&number(step.y));
      require([step.board_before_mask,step.placement_mask,step.board_after_mask].every(fits));
      const source=Number(step.source_index);require(Number.isSafeInteger(source) && !usedSources.has(source) && combined[source]===step.piece);usedSources.add(source);
      if(step.hold_decision==='none')require(active===source);
      else if(step.hold_decision==='swap'){require(p.hold_enabled&&active!==null&&held===source);held=active;}
      else if(step.hold_decision==='store'){require(p.hold_enabled&&active!==null&&held===null&&cursor===source);held=active;cursor++;}
      else if(step.hold_decision==='release-held-at-terminal'){require(p.hold_enabled&&active===null&&held===source);held=null;}
      else require(false);
      active=cursor<combined.length?cursor++:null;
      const origin=ends.findIndex(end=>source<end);require(origin>=0);usedOrigins[origin]++;
      const kind='IJLOSTZ'.indexOf(step.piece);supplyCounts[origin][kind]++;targetCounts[stage][kind]++;
      const lock=BigInt(step.placement_mask);require(countRecoveryCells(lock)===4&&!(lock&board)&&BigInt(step.board_before_mask)===board);
      const map:number[]=[];for(let y=0;y<p.height;y++)if(!deleted.has(y))map.push(y);
      let logical=0n;for(let y=0;y<p.height;y++){
        const row=(lock>>BigInt(y*10))&1023n;if(row===0n)continue;require(map[y]!==undefined);logical|=row<<BigInt(map[y]*10);
      }
      require((logical&actual[stage])===logical&&!(logical&used[stage]));
      let prefix=0;while(prefix<n.length && used[prefix]===actual[prefix])prefix++;
      for(let b=prefix;b<stage;b++){early[b]++;require(early[b]<=maximum[b]);}
      used[stage]|=logical;
      require(step.result_target===(stage>0)&&step.middle_complete===(used[0]===actual[0]));
      let full=0;for(let y=0;y<p.height;y++)if((((board|lock)>>BigInt(y*10))&1023n)===1023n)full+=2**y;
      require(step.cleared_rows===full&&step.cleared_lines===countRecoveryCells(BigInt(full)));
      board=compactRecoveryBoard(board|lock,p.height);require(BigInt(step.board_after_mask)===board);
      for(let y=0;y<p.height;y++)if(full&(2**y)){require(map[y]!==undefined);deleted.add(map[y]);}
      if(step.cleared_lines>0)b2b=step.cleared_lines===4||board===0n||step.recognized_spin;
      require(step.b2b_active===b2b&&(!p.preserve_b2b||step.cleared_lines===0||b2b));
    }
    require(used.every((m,i)=>m===actual[i])&&usedOrigins.every((count,i)=>count===n[i]));
    require(early.every((v,i)=>v===e.early_by_boundary![i]) && (e.status==='normal')===early.every(v=>v===0));
    require(e.exchange_balance.every((v,i)=>v===supplyCounts[0][i]-targetCounts[0][i]));
    require(p.allow_piece_exchange||supplyCounts.every((c,i)=>c.every((v,j)=>v===targetCounts[i][j])));
    require(hex(e.terminal_board_mask)&&BigInt(e.terminal_board_mask)===board&&board===compactRecoveryBoard(actual.reduce((a,b)=>a|b,start),p.height));
  }
}
