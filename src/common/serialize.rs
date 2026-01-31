use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

#[cfg(target_arch = "wasm32")]
use serde::Deserialize;
use serde::{Serialize, de::DeserializeOwned};

// Newtype for Arc<T>
#[cfg_attr(target_arch = "wasm32", derive(Deserialize))]
#[derive(Debug)]
pub struct ArcWrapper<T>(
    #[cfg(target_arch = "wasm32")] pub T,
    #[cfg(not(target_arch = "wasm32"))] pub Arc<T>,
);

#[cfg(not(target_arch = "wasm32"))]
impl<T> Clone for ArcWrapper<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> ArcWrapper<T> {
    pub fn new(inner: Arc<T>) -> Self {
        Self(inner)
    }

    pub fn inner(&self) -> &Arc<T> {
        &self.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> From<Arc<T>> for ArcWrapper<T> {
    fn from(value: Arc<T>) -> Self {
        Self(value)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Serialize> Serialize for ArcWrapper<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.as_ref().serialize(serializer)
    }
}

// Newtype for Vec<Arc<T>>
#[cfg_attr(target_arch = "wasm32", derive(Deserialize))]
#[derive(Debug, Clone)]
pub struct VecArcWrapper<T>(
    #[cfg(target_arch = "wasm32")] pub Vec<T>,
    #[cfg(not(target_arch = "wasm32"))] pub Vec<Arc<T>>,
);

impl<T> Default for VecArcWrapper<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> VecArcWrapper<T> {
    pub fn new(inner: Vec<Arc<T>>) -> Self {
        Self(inner)
    }

    pub fn inner(&self) -> &Vec<Arc<T>> {
        &self.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> From<Vec<Arc<T>>> for VecArcWrapper<T> {
    fn from(value: Vec<Arc<T>>) -> Self {
        Self(value)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Serialize> Serialize for VecArcWrapper<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let vec_ref: Vec<&T> = self.0.iter().map(|arc| arc.as_ref()).collect();
        vec_ref.serialize(serializer)
    }
}

// Newtype for RwLock<HashSet<Arc<T>>>
#[cfg_attr(target_arch = "wasm32", derive(Deserialize))]
#[derive(Debug)]
pub struct RwLockHashSetArcWrapper<T>(
    #[cfg(target_arch = "wasm32")] pub Vec<T>,
    #[cfg(not(target_arch = "wasm32"))] pub RwLock<HashSet<Arc<T>>>,
);

#[cfg(not(target_arch = "wasm32"))]
impl<T> RwLockHashSetArcWrapper<T> {
    pub fn new(inner: RwLock<HashSet<Arc<T>>>) -> Self {
        Self(inner)
    }

    pub fn inner(&self) -> &RwLock<HashSet<Arc<T>>> {
        &self.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Serialize> Serialize for RwLockHashSetArcWrapper<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let guard = self.0.read().unwrap();
        let vec: Vec<&T> = guard.iter().map(|arc| arc.as_ref()).collect();
        vec.serialize(serializer)
    }
}

// Newtype for RwLock<Vec<Arc<T>>>
#[cfg_attr(target_arch = "wasm32", derive(Deserialize))]
#[derive(Debug)]
pub struct RwLockVecArcWrapper<T>(
    #[cfg(target_arch = "wasm32")] pub Vec<T>,
    #[cfg(not(target_arch = "wasm32"))] pub RwLock<Vec<Arc<T>>>,
);

#[cfg(not(target_arch = "wasm32"))]
impl<T> RwLockVecArcWrapper<T> {
    pub fn new(inner: RwLock<Vec<Arc<T>>>) -> Self {
        Self(inner)
    }

    pub fn inner(&self) -> &RwLock<Vec<Arc<T>>> {
        &self.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Serialize> Serialize for RwLockVecArcWrapper<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let guard = self.0.read().unwrap();
        let vec: Vec<&T> = guard.iter().map(|arc| arc.as_ref()).collect();
        vec.serialize(serializer)
    }
}

// Newtype for RwLock<T>
#[cfg_attr(target_arch = "wasm32", derive(serde::Deserialize))]
#[derive(Debug)]
pub struct RwLockWrapper<T>(
    #[cfg(target_arch = "wasm32")] pub T,
    #[cfg(not(target_arch = "wasm32"))] pub RwLock<T>,
);

#[cfg(not(target_arch = "wasm32"))]
impl<T> RwLockWrapper<T> {
    pub fn new(inner: T) -> Self {
        Self(RwLock::new(inner))
    }

    pub fn inner(&self) -> &RwLock<T> {
        &self.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> From<RwLock<T>> for RwLockWrapper<T> {
    fn from(value: RwLock<T>) -> Self {
        Self(value)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Serialize> Serialize for RwLockWrapper<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let guard = self.0.read().unwrap();
        guard.serialize(serializer)
    }
}

#[cfg_attr(target_arch = "wasm32", derive(serde::Deserialize))]
#[derive(Debug)]
pub struct RwLockHashMapArc<T>(
    #[cfg(target_arch = "wasm32")] pub HashMap<String, T>,
    #[cfg(not(target_arch = "wasm32"))] pub RwLock<HashMap<String, Arc<T>>>,
);

#[cfg(not(target_arch = "wasm32"))]
impl<T> RwLockHashMapArc<T> {
    pub fn new(inner: RwLock<HashMap<String, Arc<T>>>) -> Self {
        Self(inner)
    }

    pub fn inner(&self) -> &RwLock<HashMap<String, Arc<T>>> {
        &self.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> From<RwLock<HashMap<String, Arc<T>>>> for RwLockHashMapArc<T> {
    fn from(value: RwLock<HashMap<String, Arc<T>>>) -> Self {
        Self(value)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Serialize> Serialize for RwLockHashMapArc<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let guard = self.0.read().unwrap();
        let map: HashMap<&String, &T> = guard.iter().map(|(k, v)| (k, v.as_ref())).collect();
        map.serialize(serializer)
    }
}
