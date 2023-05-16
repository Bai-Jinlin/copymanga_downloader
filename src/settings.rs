use config::Config;
use config::ConfigError;
use config::File;
use serde::Deserialize;
#[derive(Debug,Deserialize)]
pub struct Settings{
    pub driver:Driver,
    pub http_proxy:Option<String>
}


#[derive(Debug,Deserialize)]
pub struct Driver{
    pub driver_path:String,
    pub firefox_binary_path:String
}



impl Settings{
    pub fn new()->Result<Self,ConfigError>{
        let s=Config::builder()
        .add_source(File::with_name("./config"))
        .build()?;
        s.try_deserialize()
    }
}

mod test{
    use super::Settings;

    #[test]
    fn test(){
        let s=Settings::new().unwrap(); 
        assert_eq!(s.driver.driver_path,"./driver/firefox.exe");
        assert_eq!(s.http_proxy,Some("http://localhost:7890".to_owned()));
    }
    
}