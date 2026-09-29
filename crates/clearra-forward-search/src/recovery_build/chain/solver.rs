//! Exact product of the physical board and a compressed input language. No
//! boundary resets the held token, cursor, repayment balance or deletion map.
use super::{fields, source::Sources, RecoveryChainError as Error, RecoveryChainQuery,
    RecoveryChainStep, RecoveryChainWitness};
use crate::{board::{place_and_clear, ForwardBoard}, reachability::ReachabilityWorkspace,
    recovery_build::{RecoveryBuildError as Core, RecoveryBuildStatus, search::piece_index,
        staged::{diagram::{Diagram,Id,ALL,NONE},source::{cancelled,PIECES}}}, search::t_corner_counts};
use clearra_core_domain::{board::standard_pc_board::Board256Mask as Mask,
    execution_cancellation::ExecutionControl,piece::piece_kind::PieceKind};
use clearra_core_executor::backend::{BuildStageDomain,BuildStageDomainError};
use clearra_problem::BuildProbabilityField;
use clearra_replay::ScoringExecutionEdge;
use clearra_scoring::{b2b_preservation::BackToBackPreservationPolicy,event::SpinDetector,profile::SpinProfile};
use std::collections::HashMap;

#[derive(Clone,Copy,Debug,Eq,Hash,PartialEq)]
struct Token { index:u16,piece:u8 }
#[derive(Clone,Debug,Eq,Hash,PartialEq)]
pub(super) struct Accounting {
    pub source_used:Vec<u8>,
    pub target_counts:Vec<[u8;7]>,
    pub target_used:Vec<u8>,
    pub early:Vec<u8>,
    pub exchange:Vec<[i16;7]>,
}
impl Accounting {
    pub fn new(stages:usize)->Self {
        Self {source_used:vec![0;stages],target_counts:vec![[0;7];stages],target_used:vec![0;stages],
            early:vec![0;stages-1],exchange:vec![[0;7];stages]}
    }
    pub fn frontier(&self,demands:&[u8])->usize {
        self.target_used.iter().zip(demands).position(|(n,d)|n!=d).unwrap_or(demands.len())
    }
    pub fn lock(&self,source:usize,target:usize,piece:usize,demands:&[u8],maximum:&[usize])->Option<Self> {
        if source>=demands.len() || target>=demands.len() || piece>=7 || maximum.len()+1!=demands.len() ||
            self.source_used[source]>=demands[source] || self.target_used[target]>=demands[target] {return None;}
        let frontier=self.frontier(demands);
        if target<frontier {return None;}
        let mut next=self.clone();
        // A far-future lock counts across every still-open boundary it crosses.
        // Later-source and held tokens cannot bypass this physical count.
        for boundary in frontier..target {
            if usize::from(next.early[boundary])>=maximum[boundary] {return None;}
            next.early[boundary]=next.early[boundary].checked_add(1)?;
        }
        next.source_used[source]=next.source_used[source].checked_add(1)?;
        next.target_used[target]=next.target_used[target].checked_add(1)?;
        next.target_counts[target][piece]=next.target_counts[target][piece].checked_add(1)?;
        next.exchange[source][piece]=next.exchange[source][piece].checked_add(1)?;
        next.exchange[target][piece]=next.exchange[target][piece].checked_sub(1)?;
        Some(next)
    }
    pub fn terminal(&self,demands:&[u8],exchange_allowed:bool)->bool {
        self.target_used.as_slice()==demands && self.source_used.as_slice()==demands &&
            (exchange_allowed || self.exchange.iter().all(|counts|*counts==[0;7]))
    }
}
#[derive(Clone,Debug,Eq,Hash,PartialEq)]
struct State {
    board:ForwardBoard,
    used:Vec<Mask>,
    deleted:u32,
    language:Id,
    depth:u16,
    active:Option<Token>,
    hold:Option<Token>,
    can_hold:bool,
    accounting:Accounting,
    b2b:bool,
}
#[derive(Clone,Copy)]
enum Map { Identity, Draw(u16,usize) }
struct Action { child:State, map:Map, step:Option<RecoveryChainStep> }
struct Frame { key:State, actions:std::vec::IntoIter<Action>, waiting:Option<Map>, accepted:Id }
struct Machine { frames:Vec<Frame>, pending:Option<State>, returned:Option<Id>, result:Option<Id> }
enum Prepared { Terminal(Id), Actions(Vec<Action>) }
fn domain_error(error:BuildStageDomainError)->Error {
    match error {
        BuildStageDomainError::InvalidField=>Core::PatternDomainUnavailable,
        BuildStageDomainError::Allocation=>Core::MemoryUnavailable,
        BuildStageDomainError::Cancelled=>Core::Cancelled,
    }.into()
}

pub(super) struct Solver {
    query:RecoveryChainQuery,
    pub targets:Vec<Mask>,
    pub repair:bool,
    demands:Vec<u8>,
    maximum:Vec<usize>,
    terminal:ForwardBoard,
    domains:Vec<BuildStageDomain>,
    reach:ReachabilityWorkspace,
    profile:SpinProfile,
    root:State,
    memo:HashMap<State,Id>,
    machine:Option<Machine>,
    answer:Option<Id>,
    pub states:u128,
}
impl Solver {
    pub fn new(q:&RecoveryChainQuery,targets:Vec<Mask>,repair:bool,sources:&Sources,control:&ExecutionControl)->Result<Self,Error> {
        cancelled(control)?;
        let mut oriented=q.clone();oriented.targets=targets.clone();fields::validate(&oriented)?;
        let demands=targets.iter().map(|m|(m.count_ones()/4) as u8).collect::<Vec<_>>();
        let maximum=(0..targets.len()-1).map(|i| {
            let remaining=demands[i+1..].iter().map(|&n|usize::from(n)).sum::<usize>();
            if repair {q.early_limit.effective_max(remaining,remaining)} else {0}
        }).collect::<Vec<_>>();
        let union=targets.iter().fold(q.initial,|u,&t|u.union(t));
        let mut domains=Vec::new();
        // Relaxed geometry contexts, not occupied future blocks. Reachability
        // and spin below always use the current physical State.board.
        for &target in &targets {
            cancelled(control)?;
            let h=(0..q.height).rev().find(|&y|(0..10).any(|x|target.contains_index(u16::from(y)*10+x)))
                .map_or(1,|y|y+1);
            let clip=Mask::all_cells(u16::from(h)*10).map_err(|_|Core::BoardOutsideField)?;
            let context=union.without(target);
            let base=Mask::from_words(core::array::from_fn(|w|context.words()[w]&clip.words()[w]));
            let field=BuildProbabilityField::from_words_preserving_height(h,base.words(),target.words())
                .map_err(|_|Core::BoardOutsideField)?;
            domains.push(BuildStageDomain::compile(field,control).map_err(domain_error)?);
        }
        let (initial,_,_)=place_and_clear(10,q.height,ForwardBoard::from_mask(q.initial));
        let (terminal,_,_)=place_and_clear(10,q.height,ForwardBoard::from_mask(union));
        let root=State {board:initial,used:vec![Mask::EMPTY;targets.len()],deleted:fields::full_rows(q.initial,q.height),
            language:sources.universe,depth:0,active:None,hold:None,can_hold:true,
            accounting:Accounting::new(targets.len()),b2b:q.initial_b2b};
        let machine=Machine {frames:Vec::new(),pending:Some(root.clone()),returned:None,result:None};
        Ok(Self {query:q.clone(),targets,repair,demands,maximum,terminal,domains,
            reach:ReachabilityWorkspace::new(q.height,q.rule_profile).map_err(|_|Core::UnsupportedRuleProfile)?,
            profile:SpinProfile::builtin(q.spin_profile),root,memo:HashMap::new(),machine:Some(machine),answer:None,states:0})
    }
    pub fn result(&self)->Option<Id> {self.answer}
    pub fn advance(&mut self,fuel:usize,diagram:&mut Diagram,sources:&Sources,control:&ExecutionControl)->Result<bool,Error> {
        for _ in 0..fuel.max(1) {
            cancelled(control)?;
            if self.answer.is_some() {return Ok(true);}
            let mut machine=self.machine.take().ok_or(Error::Incomplete)?;
            self.tick(&mut machine,diagram,sources,control)?;
            if let Some(value)=machine.result {self.answer=Some(value);} else {self.machine=Some(machine);}
        }
        Ok(self.answer.is_some())
    }
    fn remember(&mut self,key:State,value:Id)->Result<(),Error> {
        self.memo.try_reserve(1).map_err(|_|Core::MemoryUnavailable)?;
        self.memo.insert(key,value);Ok(())
    }
    fn mapped(diagram:&mut Diagram,value:Id,map:Map)->Result<Id,Error> {
        Ok(match map {Map::Identity=>value,Map::Draw(level,piece)=>diagram.prepend(level,piece,value)?})
    }
    fn tick(&mut self,m:&mut Machine,diagram:&mut Diagram,sources:&Sources,control:&ExecutionControl)->Result<(),Error> {
        if let Some(key)=m.pending.take() {
            if let Some(&value)=self.memo.get(&key) {m.returned=Some(value);return Ok(());}
            self.states=self.states.checked_add(1).ok_or(Core::CounterOverflow)?;
            match self.actions(&key,diagram,sources,false,control)? {
                Prepared::Terminal(value)=> {self.remember(key,value)?;m.returned=Some(value);},
                Prepared::Actions(actions)=> {
                    m.frames.try_reserve(1).map_err(|_|Core::MemoryUnavailable)?;
                    m.frames.push(Frame {key,actions:actions.into_iter(),waiting:None,accepted:NONE});
                },
            }
            return Ok(());
        }
        if let Some(value)=m.returned.take() {
            if let Some(frame)=m.frames.last_mut() {
                let value=Self::mapped(diagram,value,frame.waiting.take().ok_or(Error::Incomplete)?)?;
                frame.accepted=diagram.union(frame.accepted,value)?;
            } else {m.result=Some(value);}
            return Ok(());
        }
        let frame=m.frames.last_mut().ok_or(Error::Incomplete)?;
        // This short circuit is valid only for aggregate coverage or one fixed
        // tiling. It never claims an all-tiling catalog from a first witness.
        if frame.accepted!=frame.key.language {
            if let Some(action)=frame.actions.next() {
                frame.waiting=Some(action.map);m.pending=Some(action.child);return Ok(());
            }
        }
        let frame=m.frames.pop().ok_or(Error::Incomplete)?;
        self.remember(frame.key,frame.accepted)?;m.returned=Some(frame.accepted);Ok(())
    }
    fn feasible(&mut self,key:&State,sources:&Sources,control:&ExecutionControl)->Result<bool,Error> {
        if self.demands.iter().enumerate().any(|(i,&n)|sources.len(i)<usize::from(n)) {return Ok(false);}
        let mut exact_total=Some([0_u8;7]);
        if usize::from(sources.end())!=self.demands.iter().map(|&n|usize::from(n)).sum::<usize>() {exact_total=None;}
        if exact_total.is_some() {
            for source in &sources.fixed_counts {
                let Some(counts)=source else {exact_total=None;break;};
                let total=exact_total.as_mut().ok_or(Error::Incomplete)?;
                for p in 0..7 {total[p]=total[p].checked_add(counts[p]).ok_or(Core::CounterOverflow)?;}
            }
        }
        if let Some(total)=&mut exact_total {
            for counts in &key.accounting.target_counts {
                for p in 0..7 {
                    let Some(rest)=total[p].checked_sub(counts[p]) else {return Ok(false)};
                    total[p]=rest;
                }
            }
        }
        for i in 0..self.targets.len() {
            let remaining=self.targets[i].without(key.used[i]);
            if remaining.is_empty() {continue;}
            let mut caps=[(remaining.count_ones()/4) as u8;7];
            if let Some(total)=exact_total {for p in 0..7 {caps[p]=caps[p].min(total[p]);}}
            if !self.query.allow_piece_exchange && sources.len(i)==usize::from(self.demands[i]) {
                if let Some(counts)=sources.fixed_counts[i] {
                    for p in 0..7 {
                        let Some(rest)=counts[p].checked_sub(key.accounting.target_counts[i][p]) else {return Ok(false)};
                        caps[p]=caps[p].min(rest);
                    }
                }
            }
            if !self.domains[i].can_complete(remaining,caps,control).map_err(domain_error)? {return Ok(false);}
        }
        Ok(true)
    }
    fn actions(&mut self,key:&State,diagram:&Diagram,sources:&Sources,trace:bool,control:&ExecutionControl)->Result<Prepared,Error> {
        cancelled(control)?;
        if key.language==NONE {return Ok(Prepared::Terminal(NONE));}
        if key.accounting.frontier(&self.demands)==self.demands.len() {
            let valid=key.board==self.terminal && key.accounting.terminal(&self.demands,self.query.allow_piece_exchange);
            return Ok(Prepared::Terminal(if valid {key.language} else {NONE}));
        }
        if !self.feasible(key,sources,control)? {return Ok(Prepared::Terminal(NONE));}
        let mut actions=Vec::new();
        if let Some(active)=key.active {
            self.placements(key,active,key.hold,if key.can_hold {"none"} else {"store"},sources,trace,&mut actions,control)?;
            if key.can_hold && self.query.hold_enabled {
                if let Some(held)=key.hold {
                    self.placements(key,held,Some(active),"swap",sources,trace,&mut actions,control)?;
                } else if key.depth<sources.end() {
                    let mut child=key.clone();child.active=None;child.hold=Some(active);child.can_hold=false;
                    actions.push(Action {child,map:Map::Identity,step:None});
                }
            }
        } else if key.depth<sources.end() {
            for piece in 0..7 {
                let language=diagram.follow(key.language,key.depth,piece);
                if language==NONE {continue;}
                let mut child=key.clone();
                child.language=language;child.active=Some(Token {index:key.depth,piece:piece as u8});child.depth+=1;
                actions.try_reserve(1).map_err(|_|Core::MemoryUnavailable)?;
                actions.push(Action {child,map:Map::Draw(key.depth,piece),step:None});
            }
        } else if let Some(held)=key.hold {
            self.placements(key,held,None,"release-held-at-terminal",sources,trace,&mut actions,control)?;
        }
        Ok(Prepared::Actions(actions))
    }
    #[allow(clippy::too_many_arguments)]
    fn placements(&mut self,key:&State,token:Token,held:Option<Token>,decision:&'static str,
        sources:&Sources,trace:bool,actions:&mut Vec<Action>,control:&ExecutionControl)->Result<(),Error> {
        let source=sources.stage_of(token.index).ok_or(Core::PatternDomainUnavailable)?;
        if key.accounting.source_used[source]>=self.demands[source] {return Ok(());}
        let piece=usize::from(token.piece);
        let kind=PIECES[piece];
        let locks=self.reach.reachable_locks(key.board,kind,true,true).to_vec();
        for lock in locks {
            cancelled(control)?;
            let Some(logical)=fields::lift(lock.mask,key.deleted,self.query.height) else {continue};
            if logical.count_ones()!=4 {continue;}
            let Some(target)=self.targets.iter().enumerate().position(|(i,t)|
                logical.without(*t).is_empty() && !logical.intersects(key.used[i])) else {continue};
            let Some(accounting)=key.accounting.lock(source,target,piece,&self.demands,&self.maximum) else {continue};
            let (board,cleared_rows,cleared_lines)=place_and_clear(10,self.query.height,
                key.board.union_for_height(lock.mask,self.query.height));
            let (corners,front)=t_corner_counts(key.board,self.query.height,kind,lock.rotation,lock.x,lock.y);
            let pc=cleared_lines>0 && board.is_empty();
            let scoring=ScoringExecutionEdge::new(0,0,kind,lock.rotation,lock.x,lock.y,cleared_lines,corners,front,
                lock.evidence.scoring(lock.rotation,lock.immobile)).with_perfect_clear(pc);
            let spin=SpinDetector::detect_scoring_edge_with_profile(scoring,self.profile).is_some();
            if self.query.preserve_b2b && !BackToBackPreservationPolicy::new(self.profile).allows(scoring) {continue;}
            let mut child=key.clone();
            child.board=board;child.used[target]=child.used[target].union(logical);
            child.deleted=fields::delete_rows(cleared_rows,key.deleted,self.query.height);
            child.active=None;child.hold=held;child.can_hold=true;child.accounting=accounting;
            child.b2b=if cleared_lines==0 {key.b2b} else {cleared_lines==4 || pc || spin};
            let step=trace.then(|| RecoveryChainStep {target_stage:target,source_stage:source,source_index:usize::from(token.index),
                piece:kind,rotation:lock.rotation.quarter_turns(),x:lock.x,y:lock.y,hold_decision:decision,
                board_before:key.board.words(),placement:lock.mask.words(),board_after:board.words(),
                logical_placement:logical.words(),cleared_rows,cleared_lines,recognized_spin:spin,
                b2b_active:child.b2b,completed_stages:child.accounting.frontier(&self.demands)});
            actions.try_reserve(1).map_err(|_|Core::MemoryUnavailable)?;
            actions.push(Action {child,map:Map::Identity,step});
        }
        Ok(())
    }
    fn accepts(diagram:&Diagram,mut language:Id,depth:u16,queue:&[PieceKind])->bool {
        for (i,&p) in queue.iter().enumerate().skip(usize::from(depth)) {
            language=diagram.follow(language,i as u16,piece_index(p));
        }
        language==ALL
    }
    pub fn witness(&mut self,diagram:&mut Diagram,sources:&Sources,queues:Vec<Vec<PieceKind>>,status:RecoveryBuildStatus,
        control:&ExecutionControl)->Result<RecoveryChainWitness,Error> {
        if self.answer.is_none() || self.answer==Some(NONE) {return Err(Error::Incomplete);}
        let flat=queues.iter().flatten().copied().collect::<Vec<_>>();
        if !Self::accepts(diagram,self.answer.ok_or(Error::Incomplete)?,0,&flat) {return Err(Core::PatternDomainUnavailable.into());}
        let mut key=self.root.clone();let mut steps=Vec::new();
        loop {
            cancelled(control)?;
            match self.actions(&key,diagram,sources,true,control)? {
                Prepared::Terminal(value)=> {
                    if !Self::accepts(diagram,value,key.depth,&flat) ||
                        (status==RecoveryBuildStatus::Recovery && key.accounting.early.iter().all(|&n|n==0)) {
                        return Err(Core::PatternDomainUnavailable.into());
                    }
                    return Ok(RecoveryChainWitness {status,queues,targets:self.targets.iter().map(|t|t.words()).collect(),
                        early_by_boundary:key.accounting.early.iter().map(|&n|usize::from(n)).collect(),
                        exchange_by_stage:key.accounting.exchange,steps,terminal_board:key.board.words()});
                },
                Prepared::Actions(actions)=> {
                    let mut selected=None;
                    for action in actions {
                        let Some(&value)=self.memo.get(&action.child) else {continue};
                        let value=Self::mapped(diagram,value,action.map)?;
                        if Self::accepts(diagram,value,key.depth,&flat) {selected=Some(action);break;}
                    }
                    let next=selected.ok_or(Core::PatternDomainUnavailable)?;
                    if let Some(step)=next.step {steps.push(step);}
                    key=next.child;
                },
            }
        }
    }
}
