//! Physical verification of one complete multi-stage tiling against symbolic
//! source choices. Holds retain global input indices across every boundary.
use super::*;
use super::geometry::Plan;
use super::super::staged::source::{cancelled,PIECES};
use crate::{board::{ForwardBoard,place_and_clear},reachability::{ReachabilityWorkspace,ReachableLock},search::t_corner_counts};
use clearra_replay::ScoringExecutionEdge;
use clearra_scoring::{event::SpinDetector,b2b_preservation::BackToBackPreservationPolicy,profile::SpinProfile};
use clearra_core_domain::piece::piece_kind::PieceKind;
use std::{collections::HashMap,sync::Arc};
#[derive(Clone,Copy,Debug,Eq,Hash,PartialEq)]
struct Position {used:u64,deleted:u32,board:ForwardBoard,b2b:bool}
#[derive(Clone,Copy,Debug,Eq,Hash,PartialEq)]
struct Token {index:u16,piece:u8}
#[derive(Clone,Debug,Eq,Hash,PartialEq)]
struct Key {
    position:Position,source:Id,depth:u16,active:Option<Token>,hold:Option<Token>,can_hold:bool,
    source_used:Vec<u8>,early:Vec<u8>,exchange:Vec<[i16;7]>,
}
#[derive(Clone,Copy)]
struct Edge {tile:usize,position:Position,lock:ReachableLock,rows:u32,lines:u8,spin:bool}
#[derive(Clone,Copy)]
enum Map {Identity,Draw{level:u16,piece:u8},Place{from:Position,edge:Edge,token:Token,decision:&'static str}}
#[derive(Clone)]
struct Action {key:Key,map:Map}
struct Frame {key:Key,actions:std::vec::IntoIter<Action>,waiting:Option<Map>,accepted:[Id;2]}
pub(super) struct Solver {
    pub plan:Plan,pub source:source::Source,pub diagram:Diagram,pub normal:Id,pub recovery:Id,pub states:u128,
    query:RecoveryChainQuery,root:Key,reach:ReachabilityWorkspace,profile:SpinProfile,
    pending:Option<Key>,returned:Option<[Id;2]>,frames:Vec<Frame>,memo:HashMap<Key,[Id;2]>,
    edges:HashMap<(Position,u8),Arc<[Edge]>>,done:bool,
}
impl Solver {
    pub fn new(query:RecoveryChainQuery,plan:Plan,source:source::Source,diagram:Diagram)->Result<Self,Error>{
        let (board,deleted,_)=place_and_clear(10,query.height,ForwardBoard::from_mask(query.initial));
        let root=Key{position:Position{used:0,deleted,board,b2b:query.initial_b2b},source:source.universe,depth:0,
            active:None,hold:None,can_hold:true,source_used:vec![0;query.targets.len()],
            early:vec![0;query.targets.len()-1],exchange:vec![[0;7];query.targets.len()]};
        let reach=ReachabilityWorkspace::new(query.height,query.rule_profile).map_err(|_|Error::UnsupportedRuleProfile)?;
        let profile=SpinProfile::builtin(query.spin_profile);
        Ok(Self{query,plan,source,diagram,root:root.clone(),reach,profile,pending:Some(root),returned:None,
            frames:Vec::new(),memo:HashMap::new(),edges:HashMap::new(),done:false,normal:NONE,recovery:NONE,states:0})
    }
    pub fn advance(&mut self,fuel:usize,c:&ExecutionControl)->Result<bool,Error>{
        for _ in 0..fuel.max(1){
            cancelled(c)?;
            if self.done{return Ok(true);}
            if let Some(key)=self.pending.take(){
                if let Some(&value)=self.memo.get(&key){self.returned=Some(value);continue;}
                self.states=self.states.checked_add(1).ok_or(Error::CounterOverflow)?;
                match self.actions(&key,c)? {
                    Prepared::Terminal(value)=>{self.remember(key,value)?;self.returned=Some(value);},
                    Prepared::Actions(actions)=>{
                        self.frames.try_reserve(1).map_err(|_|Error::MemoryUnavailable)?;
                        self.frames.push(Frame{key,actions:actions.into_iter(),waiting:None,accepted:[NONE;2]});
                    }
                }
            }else if let Some(mut value)=self.returned.take(){
                if self.frames.is_empty(){self.normal=value[0];self.recovery=self.diagram.difference(value[1],value[0])?;self.done=true;continue;}
                let map=self.frames.last_mut().unwrap().waiting.take().ok_or(Error::PatternDomainUnavailable)?;
                for id in &mut value{*id=self.map(*id,map)?;}
                let frame=self.frames.last_mut().unwrap();
                for (to,id) in frame.accepted.iter_mut().zip(value){*to=self.diagram.union(*to,id)?;}
            }else{
                let frame=self.frames.last_mut().ok_or(Error::PatternDomainUnavailable)?;
                let stop=frame.accepted[0]==frame.key.source ||
                    (frame.key.early.iter().any(|&n|n>0) && frame.accepted[1]==frame.key.source);
                if let Some(action)=if stop{None}else{frame.actions.next()}{
                    frame.waiting=Some(action.map);self.pending=Some(action.key);
                }else{
                    let frame=self.frames.pop().unwrap();
                    self.remember(frame.key,frame.accepted)?;self.returned=Some(frame.accepted);
                }
            }
        }
        Ok(self.done)
    }
    fn remember(&mut self,key:Key,value:[Id;2])->Result<(),Error>{
        self.memo.try_reserve(1).map_err(|_|Error::MemoryUnavailable)?;
        self.memo.insert(key,value);Ok(())
    }
    fn map(&mut self,value:Id,map:Map)->Result<Id,Error>{match map{
        Map::Draw{level,piece}=>self.diagram.prepend(level,usize::from(piece),value),_=>Ok(value)
    }}
    fn actions(&mut self,key:&Key,c:&ExecutionControl)->Result<Prepared,Error>{
        if key.source==NONE{return Ok(Prepared::Terminal([NONE;2]));}
        if key.position.used==*self.plan.prefix_bits.last().ok_or(Error::PatternDomainUnavailable)? {
            if key.position.board!=self.plan.terminal || key.source_used!=self.plan.demand ||
                (!self.query.allow_piece_exchange && key.exchange.iter().any(|b|*b!=[0;7])) {
                return Ok(Prepared::Terminal([NONE;2]));
            }
            return Ok(Prepared::Terminal(if key.early.iter().any(|&n|n>0){[NONE,key.source]}else{[key.source,NONE]}));
        }
        let mut actions=Vec::new();
        if let Some(active)=key.active {
            self.placements(key,active,key.hold,if key.can_hold{"none"}else{"store"},&mut actions,c)?;
            if self.query.hold_enabled && key.can_hold {
                if let Some(held)=key.hold{self.placements(key,held,Some(active),"swap",&mut actions,c)?;}
                else if key.depth<self.source.end {
                    let mut child=key.clone();child.active=None;child.hold=Some(active);child.can_hold=false;
                    actions.push(Action{key:child,map:Map::Identity});
                }
            }
        }else if key.depth<self.source.end {
            for p in 0..7 {
                let source=self.diagram.follow(key.source,key.depth,p);
                if source==NONE{continue;}
                let mut child=key.clone();child.source=source;child.depth+=1;
                child.active=Some(Token{index:key.depth,piece:p as u8});
                actions.push(Action{key:child,map:Map::Draw{level:key.depth,piece:p as u8}});
            }
        }else if let Some(held)=key.hold {self.placements(key,held,None,"release-held-at-terminal",&mut actions,c)?;}
        Ok(Prepared::Actions(actions))
    }
    fn placements(&mut self,key:&Key,token:Token,hold:Option<Token>,decision:&'static str,out:&mut Vec<Action>,c:&ExecutionControl)->Result<(),Error>{
        let origin=self.source.origin(token.index);
        if key.source_used[origin]>=self.plan.demand[origin]{return Ok(());}
        for edge in self.physical_edges(key.position,token.piece,c)?.iter().copied(){
            let stage=self.plan.tiles[edge.tile].stage;
            let mut child=key.clone();let mut permitted=true;
            // One quota per unfinished prefix boundary, independent of where
            // the held/active token originated. A skipped stage is not complete.
            for b in 0..stage {
                if !self.plan.complete(key.position.used,b){
                    let maximum=self.plan.demand[b+1..].iter().map(|&n|usize::from(n)).sum::<usize>();
                    let limit=self.query.early_limit.effective_max(maximum,maximum);
                    if usize::from(child.early[b])>=limit {permitted=false;break;}
                    child.early[b]+=1;
                }
            }
            if !permitted{continue;}
            child.position=edge.position;child.active=None;child.hold=hold;child.can_hold=true;
            child.source_used[origin]+=1;
            child.exchange[origin][usize::from(token.piece)]+=1;
            child.exchange[stage][usize::from(token.piece)]-=1;
            out.try_reserve(1).map_err(|_|Error::MemoryUnavailable)?;
            out.push(Action{key:child,map:Map::Place{from:key.position,edge,token,decision}});
        }
        Ok(())
    }
    fn physical_edges(&mut self,pos:Position,piece:u8,c:&ExecutionControl)->Result<Arc<[Edge]>,Error>{
        if let Some(edges)=self.edges.get(&(pos,piece)){return Ok(Arc::clone(edges));}
        cancelled(c)?;
        let height=self.query.height;let kind=PIECES[usize::from(piece)];
        let map=(0..height).filter(|&y|pos.deleted&(1_u32<<y)==0).collect::<Vec<_>>();
        let locks=self.reach.reachable_locks(pos.board,kind,true,true).to_vec();
        let mut edges=Vec::new();
        for lock in locks{
            cancelled(c)?;
            let mut logical=Mask::EMPTY;let mut valid=true;
            for y in 0..height {
                let row=lock.mask.row_bits(10,y);if row==0{continue;}
                let Some(&original)=map.get(usize::from(y))else{valid=false;break;};
                for x in 0..10_u16{if row&(1<<x)!=0{logical=logical.union(Mask::singleton(u16::from(original)*10+x).map_err(|_|Error::BoardOutsideField)?);}}
            }
            if !valid{continue;}
            let Some(tile)=self.plan.tiles.iter().enumerate().position(|(i,t)|pos.used&(1_u64<<i)==0 && t.piece==piece && t.cells==logical)else{continue;};
            let (board,rows,lines)=place_and_clear(10,height,pos.board.union_for_height(lock.mask,height));
            let (corners,front)=t_corner_counts(pos.board,height,kind,lock.rotation,lock.x,lock.y);
            let pc=lines>0 && board.is_empty();
            let scoring=ScoringExecutionEdge::new(0,0,kind,lock.rotation,lock.x,lock.y,lines,corners,front,lock.evidence.scoring(lock.rotation,lock.immobile)).with_perfect_clear(pc);
            let spin=SpinDetector::detect_scoring_edge_with_profile(scoring,self.profile).is_some();
            if self.query.preserve_b2b && !BackToBackPreservationPolicy::new(self.profile).allows(scoring){continue;}
            let mut deleted=pos.deleted;
            for (physical,&original) in map.iter().enumerate(){if rows&(1_u32<<physical)!=0{deleted|=1_u32<<original;}}
            let position=Position{used:pos.used|(1_u64<<tile),deleted,board,b2b:if lines==0{pos.b2b}else{lines==4||pc||spin}};
            edges.try_reserve(1).map_err(|_|Error::MemoryUnavailable)?;
            edges.push(Edge{tile,position,lock,rows,lines,spin});
        }
        let edges:Arc<[Edge]>=edges.into();
        self.edges.try_reserve(1).map_err(|_|Error::MemoryUnavailable)?;
        self.edges.insert((pos,piece),Arc::clone(&edges));Ok(edges)
    }
    pub fn witness(&mut self,_language:Id,patterns:Vec<usize>,queues:Vec<Vec<PieceKind>>,c:&ExecutionControl)->Result<RecoveryChainWitness,Error>{
        let queue=queues.iter().flatten().copied().collect::<Vec<_>>();
        let mut key=self.root.clone();let mut steps=Vec::new();let mut stages=Vec::new();
        let category=if self.source.accepts(&self.diagram,self.normal,0,&queue){0}else{1};
        loop {
            cancelled(c)?;
            match self.actions(&key,c)?{
                Prepared::Terminal(ids)=>{
                    if !self.source.accepts(&self.diagram,ids[category],key.depth,&queue){return Err(Error::PatternDomainUnavailable);}
                    return Ok(RecoveryChainWitness {queues:queues.iter().map(|q|q.iter().map(|p|p.as_ascii()).collect()).collect(),patterns,
                        targets:self.plan.targets.clone(),steps,step_stages:stages,early_counts:key.early.iter().map(|&n|usize::from(n)).collect(),
                        exchange:key.exchange,terminal_board:key.position.board.words()});
                },
                Prepared::Actions(actions)=>{
                    let mut selected=None;
                    for action in actions {
                        if let Some(value)=self.memo.get(&action.key).copied(){
                            let id=self.map(value[category],action.map)?;
                            if self.source.accepts(&self.diagram,id,key.depth,&queue){selected=Some(action);break;}
                        }
                    }
                    let action=selected.ok_or(Error::PatternDomainUnavailable)?;
                    if let Map::Place{from,edge,token,decision}=action.map {
                        let tile=self.plan.tiles[edge.tile];
                        stages.push(tile.stage);
                        steps.push(RecoveryBuildStep {source_index:usize::from(token.index),result_target:tile.stage+1==self.plan.targets.len(),
                            piece:PIECES[usize::from(token.piece)],rotation:edge.lock.rotation.quarter_turns(),x:edge.lock.x,y:edge.lock.y,
                            hold_decision:decision,board_before:from.board.words(),placement:edge.lock.mask.words(),board_after:edge.position.board.words(),
                            cleared_rows:edge.rows,cleared_lines:edge.lines,recognized_spin:edge.spin,b2b_active:edge.position.b2b,
                            middle_complete:self.plan.complete(edge.position.used,0),
                            logical_cells:(0..self.query.height).map(|y|ForwardBoard::from_mask(tile.cells).row_bits(10,y)).collect()});
                    }
                    key=action.key;
                }
            }
        }
    }
}
enum Prepared {Terminal([Id;2]),Actions(Vec<Action>)}
