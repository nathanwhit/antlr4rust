use std::borrow::{Borrow, BorrowMut};
use std::borrow::Cow::{Borrowed, Owned};
use std::cell::Cell;
use std::marker::{PhantomData, Unsize};
use std::ops::{CoerceUnsized, Deref};
use std::sync::atomic::AtomicIsize;

use typed_arena::Arena;

use crate::char_stream::CharStream;
use crate::token::{CommonToken, OwningToken, TOKEN_INVALID_TYPE};
use crate::token::Token;

lazy_static! {
    pub static ref CommonTokenFactoryDEFAULT: Box<CommonTokenFactory> =
        Box::new(CommonTokenFactory{});
}

lazy_static! {
    pub static ref INVALID_OWNING:Box<OwningToken> = Box::new(OwningToken{
        token_type: TOKEN_INVALID_TYPE,
        channel: 0,
        start: -1,
        stop: -1,
        token_index: AtomicIsize::new(-1),
        line: -1,
        column: -1,
        text: "<invalid>".to_owned(),
        read_only: true,
    });
    pub static ref INVALID_COMMON:Box<CommonToken<'static>> = Box::new(CommonToken{
        token_type: TOKEN_INVALID_TYPE,
        channel: 0,
        start: -1,
        stop: -1,
        token_index: AtomicIsize::new(-1),
        line: -1,
        column: -1,
        text: Borrowed("<invalid>"),
        read_only: true,
    });

}

// todo remove redundant allocation for arenas

/// Trait for creating tokens
pub trait TokenFactory<'a> {
    /// type of tokens emitted by this factory
    type Inner: Token + ?Sized + Unsize<dyn Token + 'a> + 'a;
    type Tok: Borrow<Self::Inner> + Clone + 'a;

    fn create<'b: 'a>(&'a self,
                      source: Option<&mut dyn CharStream<'b>>,
                      ttype: isize,
                      text: Option<String>,
                      channel: isize,
                      start: isize,
                      stop: isize,
                      line: isize,
                      column: isize,
    ) -> Self::Tok;

    fn create_invalid() -> Self::Tok;
}

#[derive(Default)]
pub struct CowTokenFactory;

impl<'a> TokenFactory<'a> for CowTokenFactory {
    type Inner = CommonToken<'a>;
    type Tok = Box<Self::Inner>;

    fn create<'b: 'a>(&'a self,
                      source: Option<&mut dyn CharStream<'b>>,
                      ttype: isize,
                      text: Option<String>,
                      channel: isize,
                      start: isize,
                      stop: isize,
                      line: isize,
                      column: isize,
    ) -> Self::Tok {
        let text = match (text, source) {
            (Some(t), _) => Owned(t),

            (None, Some(x)) => {
                let t = if stop >= x.size() || start >= x.size() { "<EOF>" } else { x.get_text(start, stop) };
                Borrowed(t)
            }
            _ => Borrowed("")
        };
        Box::new(CommonToken {
            token_type: ttype,
            channel,
            start,
            stop,
            token_index: AtomicIsize::new(-1),
            line,
            column,
            text,
            read_only: false,
        })
    }

    fn create_invalid() -> Self::Tok {
        INVALID_COMMON.clone()
    }
}

#[derive(Default)]
pub struct CommonTokenFactory {}

impl Default for &'_ CommonTokenFactory {
    fn default() -> Self {
        &**CommonTokenFactoryDEFAULT
    }
}

impl<'a> TokenFactory<'a> for CommonTokenFactory {
    type Inner = OwningToken;
    type Tok = Box<Self::Inner>;

    fn create<'b: 'a>(&'a self,
                      source: Option<&mut dyn CharStream<'b>>,
                      ttype: isize,
                      text: Option<String>,
                      channel: isize,
                      start: isize,
                      stop: isize,
                      line: isize,
                      column: isize,
    ) -> Self::Tok {
        let text = match (text, source) {
            (Some(t), _) => t,

            (None, Some(x)) => {
                let t = if stop >= x.size() || start >= x.size() { "<EOF>" } else { x.get_text(start, stop) };
                t.to_owned()
            }
            _ => String::new()
        };
        Box::new(OwningToken {
            token_type: ttype,
            channel,
            start,
            stop,
            token_index: AtomicIsize::new(-1),
            line,
            column,
            text,
            read_only: false,
        })
    }

    fn create_invalid() -> Self::Tok {
        INVALID_OWNING.clone()
    }
}

// pub struct DynFactory<'input,TF:TokenFactory<'input>>(TF) where TF::Tok:CoerceUnsized<Box<dyn Token+'input>>;
// impl <'input,TF:TokenFactory<'input>> TokenFactory<'input> for DynFactory<'input,TF>
// where TF::Tok:CoerceUnsized<Box<dyn Token+'input>>
// {
//
// }

pub type ArenaCommonFactory<'a> = ArenaFactory<'a, CommonTokenFactory, OwningToken>;
pub type ArenaCowFactory<'a> = ArenaFactory<'a, CowTokenFactory, CommonToken<'a>>;

/// This is a wrapper for Token factory that allows to allocate tokens in separate arena.
/// It will allow to significantly improve performance by passing Token references everywhere.
// Box is used here because it is almost always should be used for token factory
pub struct ArenaFactory<'input, TF: TokenFactory<'input, Tok=Box<T>, Inner=T>, T: Token + Clone + 'input> {
    arena: Arena<T>,
    factory: TF,
    pd: PhantomData<&'input str>,
}

impl<'input, TF: TokenFactory<'input, Tok=Box<T>, Inner=T> + Default, T: Token + Clone + 'input> Default for ArenaFactory<'input, TF, T> {
    fn default() -> Self {
        Self {
            arena: Default::default(),
            factory: Default::default(),
            pd: Default::default(),
        }
    }
}


impl<'input, TF, T> TokenFactory<'input> for ArenaFactory<'input, TF, T>
    where TF: TokenFactory<'input, Tok=Box<T>, Inner=T>,
          T: Token + Clone + 'input,
          for<'a> &'a T: Default
{
    type Inner = T;
    type Tok = &'input T;

    fn create<'b: 'input>(&'input self,
                          source: Option<&mut dyn CharStream<'b>>,
                          ttype: isize,
                          text: Option<String>,
                          channel: isize,
                          start: isize,
                          stop: isize,
                          line: isize,
                          column: isize,
    ) -> Self::Tok {
        let token = self.factory
            .create(source, ttype, text, channel, start, stop, line, column);
        self.arena.alloc(*token)
    }

    fn create_invalid() -> &'input T {
        <&T as Default>::default()
    }
}

pub trait TokenAware<'input> {
    type TF: TokenFactory<'input> + 'input;
}
