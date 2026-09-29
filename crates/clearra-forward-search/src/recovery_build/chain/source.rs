//! Independent per-stage probability spaces, concatenated as one reduced input
//! language. Weighting visits shared suffixes once; never materializes tuples.
use super::*;
use super::super::staged::{diagram::ALL, source::{cancelled, segment, PIECES}};
use clearra_supply::{pattern_universe::{MaterializedPatternUniverse,pattern_universe_materializer::PatternUniverseMaterializer},queue::queue_pattern_expression::QueuePatternExpression};
use clearra_core_domain::piece::piece_kind::PieceKind;
use std::{sync::Arc,collections::HashMap};
#[derive(Clone)]
pub(super) struct Source {
    pub universes: Vec<Arc<MaterializedPatternUniverse>>,
    pub starts: Vec<u16>,
    pub end:u16,
    pub universe:Id,
    pub blueprint:Diagram,
    pub counts:Vec<Option<[u8;7]>>,
    pub possible:u128,
    uniform:bool,
}
impl Source {
    pub fn new(query:&RecoveryChainQuery,diagram:&mut Diagram,control:&ExecutionControl)->Result<Self,Error>{
        let mut universes=Vec::new(); let mut starts=Vec::new();let mut counts=Vec::new();
        let mut end=0_u16;let mut possible=1_u128;let mut universe=ALL;let mut uniform=true;
        for text in &query.supplies {
            cancelled(control)?;
            let expression=QueuePatternExpression::parse(text,0).map_err(|_|Error::InvalidSupplyPattern)?;
            let u=PatternUniverseMaterializer::queue_pattern_expression(&expression,0).map_err(|_|Error::PatternDomainUnavailable)?;
            starts.push(end);
            let len=u16::try_from(u.sequence_len_at(0)).map_err(|_|Error::CounterOverflow)?;
            let (root,fixed,compact)=segment(diagram,&u,end,control)?;
            counts.push(fixed);uniform &= compact;
            end=end.checked_add(len).ok_or(Error::CounterOverflow)?;
            possible=possible.checked_mul(u.pattern_count() as u128).ok_or(Error::CounterOverflow)?;
            universe=diagram.intersect(universe,root)?;
            universes.push(Arc::new(u));
        }
        if diagram.count(universe,0,end)? != possible {return Err(Error::PatternDomainUnavailable);}
        Ok(Self {universes,starts,end,universe,blueprint:diagram.clone(),counts,possible,uniform})
    }
    pub fn origin(&self,index:u16)->usize {self.starts.partition_point(|&s|s<=index)-1}
    pub fn length(&self,stage:usize)->usize {
        usize::from(self.starts.get(stage+1).copied().unwrap_or(self.end)-self.starts[stage])
    }
    fn follow(&self,d:&Diagram,mut id:Id,stage:usize,queue:&[PieceKind])->Id {
        for (n,&p) in queue.iter().enumerate() {id=d.follow(id,self.starts[stage]+n as u16,super::super::search::piece_index(p));}
        id
    }
    pub fn first(&self,d:&Diagram,mut id:Id,c:&ExecutionControl)->Result<(Vec<usize>,Vec<Vec<PieceKind>>),Error>{
        if id==NONE {return Err(Error::PatternDomainUnavailable);}
        let mut ranks=Vec::new();let mut queues=Vec::new();
        for (s,u) in self.universes.iter().enumerate() {
            let mut found=None;
            for i in 0..u.pattern_count() {
                cancelled(c)?;let queue=u.sequence_at(i).to_vec();let next=self.follow(d,id,s,&queue);
                if next!=NONE {found=Some((i,queue,next));break;}
            }
            let (rank,queue,next)=found.ok_or(Error::PatternDomainUnavailable)?;
            ranks.push(rank);queues.push(queue);id=next;
        }
        if id!=ALL {return Err(Error::PatternDomainUnavailable);}
        Ok((ranks,queues))
    }
    pub fn measure(&self,d:&mut Diagram,id:Id,c:&ExecutionControl)->Result<(u128,f64),Error>{
        if self.uniform {let n=d.count(id,0,self.end)?;return Ok((n,n as f64/self.possible as f64));}
        fn visit(s:&Source,d:&Diagram,id:Id,stage:usize,memo:&mut HashMap<(usize,Id),(u128,f64)>,c:&ExecutionControl)->Result<(u128,f64),Error>{
            cancelled(c)?;
            if id==NONE {return Ok((0,0.0));}
            if stage==s.universes.len() {return if id==ALL {Ok((1,1.0))} else {Err(Error::PatternDomainUnavailable)};}
            if let Some(v)=memo.get(&(stage,id)){return Ok(*v);}
            let u=&s.universes[stage];let mut n=0_u128;let mut sum=0.0;let mut correction=0.0;
            for i in 0..u.pattern_count() {
                let next=s.follow(d,id,stage,&u.sequence_at(i));
                let value=visit(s,d,next,stage+1,memo,c)?;
                n=n.checked_add(value.0).ok_or(Error::CounterOverflow)?;
                let y=value.1*u.weight_at(i).get()-correction;let total=sum+y;correction=(total-sum)-y;sum=total;
            }
            memo.try_reserve(1).map_err(|_|Error::MemoryUnavailable)?;
            memo.insert((stage,id),(n,sum));Ok((n,sum))
        }
        visit(self,d,id,0,&mut HashMap::new(),c)
    }
    pub fn accepts(&self,d:&Diagram,mut id:Id,depth:u16,queue:&[PieceKind])->bool {
        for (i,&p) in queue.iter().enumerate().skip(usize::from(depth)){id=d.follow(id,i as u16,PIECES.iter().position(|x|*x==p).unwrap());}
        id==ALL
    }
}
