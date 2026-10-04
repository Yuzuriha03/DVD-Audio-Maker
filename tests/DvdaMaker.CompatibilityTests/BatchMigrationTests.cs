using System.Security.Cryptography;
using DvdaMaker.Processes;
using DvdaMaker.SurcodeTool;

internal static class BatchMigrationTests
{
    private static void Require(bool value, string message)
    { if (!value) throw new InvalidDataException(message); }
    public static void Run()
    {
        var previous = Environment.GetEnvironmentVariable("DVDA_RUST_MODE");
        var root = Path.Combine(Path.GetTempPath(), "dvda-batch-中文-日本語-🎵-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        try
        {
            var input = Path.Combine(root,"input.wav"); MlpEncoderTests.WriteWave(input,96000,24,2,19217);
            var surround = Path.Combine(root,"surround.wav"); MlpEncoderTests.WriteWave(surround,48000,24,6,9617);
            var flac = Path.Combine(root,"input.flac");
            Environment.SetEnvironmentVariable("DVDA_RUST_MODE","managed");
            var conversion = new ProcessRunner().RunAsync(new ProcessRequest { FileName = BuiltinMedia.Converter,
                Arguments = ["-i",input,"-c:a","flac",flac] }).GetAwaiter().GetResult();
            Require(conversion.Succeeded,"FLAC fixture generation failed");
            var cases = 0;
            foreach (var rate in new[] {44100,48000,88200,96000,176400,192000})
            foreach (var bits in new[] {16,20,24})
            {
                var sources = rate > 96000 ? new[] {input,flac,input,flac} : new[] {input,flac,surround,input};
                var reference = Run("managed", Job("reference-"+cases,sources,rate,bits,1));
                foreach(var jobs in new[] {1,4})
                {
                    var actual = Run("rust",Job("actual-"+cases+"-"+jobs,sources,rate,bits,jobs));
                    Require(reference.SequenceEqual(actual),$"Batch bytes differ: {rate}/{bits}/{jobs}");
                }
                cases++;
            }
            foreach(var scenario in new[] {"empty","bits","rate","missing","duplicate","unsafe","duration","metadata","existing","malformed","output-directory","pre-cancel","cancel"})
            {
                var reference=Failure("managed",scenario);var actual=Failure("rust",scenario);
                Require(reference==actual,$"Batch failure {scenario}: {reference} / {actual}");
            }
            Console.WriteLine($"PASS {cases} batch profiles: old host / Rust serial / Rust parallel complete file hashes");

            SurcodeEncodingJob Job(string name,string[] sources,int rate,int bits,int jobs) => new()
            {
                FfmpegExecutable=BuiltinMedia.Converter, TemporaryDirectory=Path.Combine(root,name,"temp"),
                OutputDirectory=Path.Combine(root,name,"output"), SampleRate=rate,Bits=bits,Jobs=jobs,
                Tracks=sources.Select((source,index)=>new SurcodeEncodingTrack {SourcePath=source,WorkName="track"+index,
                    DisplayName="曲目 日本語 "+index,DurationSeconds=0.2,SourceSampleRate=rate,SourceBits=24}).ToArray()
            };
            string[] Run(string mode,SurcodeEncodingJob job)
            {
                Environment.SetEnvironmentVariable("DVDA_RUST_MODE",mode);
                new SurcodeBatchEncoder(new ProcessRunner()).RunAsync(job,CancellationToken.None).GetAwaiter().GetResult();
                Require(!Directory.EnumerateFileSystemEntries(job.TemporaryDirectory).Any(),"Batch work directory leaked");
                return job.Tracks.Select(track=>Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(Path.Combine(job.OutputDirectory,track.WorkName+".mlp"))))).ToArray();
            }
            string Failure(string mode,string scenario)
            {
                var job=Job(mode+"-"+scenario,[input],48000,24,1);
                var track=job.Tracks[0];
                if(scenario=="empty")job=job with {Tracks=[]};
                if(scenario=="bits")job=job with {Bits=32};
                if(scenario=="rate")job=job with {SampleRate=8000};
                if(scenario=="missing")job=job with {Tracks=[track with {SourcePath=Path.Combine(root,"absent.wav")}]};
                if(scenario=="duplicate")job=job with {Tracks=[track,track with {WorkName="TRACK0"}]};
                if(scenario=="unsafe")job=job with {Tracks=[track with {WorkName="../escape"}]};
                if(scenario=="duration")job=job with {Tracks=[track with {DurationSeconds=-1}]};
                if(scenario=="metadata")job=job with {MetadataContext=Path.Combine(root,"absent.ctx")};
                if(scenario=="malformed")
                {
                    var path=Path.Combine(root,mode+"-bad.wav");File.WriteAllText(path,"broken");
                    job=job with {Tracks=[track with {SourcePath=path}]};
                }
                if(scenario=="cancel")
                {
                    var path=Path.Combine(root,mode+"-long.wav");MlpEncoderTests.WriteWave(path,48000,24,2,960017);
                    job=job with {Tracks=[track with {SourcePath=path}]};
                }
                var output=Path.Combine(job.OutputDirectory,"track0.mlp");
                if(scenario is "existing" or "output-directory")
                {
                    Directory.CreateDirectory(job.OutputDirectory);
                    if(scenario=="existing")File.WriteAllText(output,"preserve");else Directory.CreateDirectory(output);
                }
                Environment.SetEnvironmentVariable("DVDA_RUST_MODE",mode);
                using var token=new CancellationTokenSource();if(scenario=="pre-cancel")token.Cancel();
                var task=new SurcodeBatchEncoder(new ProcessRunner()).RunAsync(job,token.Token);
                if(scenario=="cancel")
                {
                    for(var i=0;i<1000&&!task.IsCompleted;i++)
                    {
                        if(Directory.Exists(job.TemporaryDirectory)&&Directory.EnumerateFiles(job.TemporaryDirectory,"*",SearchOption.AllDirectories).Any())break;
                        Thread.Sleep(2);
                    }
                    Require(!task.IsCompleted,"Cancellation fixture completed too early");token.Cancel();
                }
                var type="SUCCESS";
                try{task.GetAwaiter().GetResult();}catch(Exception error){type=error is OperationCanceledException?nameof(OperationCanceledException):error.GetType().Name;}
                Require(type!="SUCCESS","Invalid batch succeeded: "+scenario);
                if(Directory.Exists(job.TemporaryDirectory))Require(!Directory.EnumerateFileSystemEntries(job.TemporaryDirectory).Any(),"Failed work directory leaked");
                var state=File.Exists(output)?Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(output))):Directory.Exists(output)?"DIRECTORY":"MISSING";
                return type+":"+state;
            }
        }
        finally{Environment.SetEnvironmentVariable("DVDA_RUST_MODE",previous);Directory.Delete(root,true);}
    }
}
